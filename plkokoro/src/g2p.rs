//! Backend fonemizacji (tekst -> IPA). Domyślny: `phonemis_runner` jako podproces.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use regex::Regex;

use crate::cancel::CancelToken;
use crate::error::{Error, Result};

/// Backend fonemizacji. Własny podajesz w `Config::g2p`; `Model` zamknie go w `unload`.
/// Backend dostaje zwykły tekst (jedno zdanie lub jego kawałek): wstawki `[tekst](/ipa/)` są obsługiwane wcześniej.
pub trait G2p: Send + Sync {
    fn phonemize(&self, text: &str, cancel: &CancelToken) -> Result<String>;

    /// Fonemizuje wiele kawałków naraz (wynik w tej samej kolejności). Domyślnie po kolei; backend runnera robi to
    /// równolegle.
    fn phonemize_many(&self, texts: &[String], cancel: &CancelToken) -> Result<Vec<String>> {
        texts.iter().map(|t| self.phonemize(t, cancel)).collect()
    }

    fn close(&self) -> Result<()> {
        Ok(())
    }
}

static ANSI_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());

/// Odrzuca brakujące wagi oraz wskaźniki Git LFS (~130 B zamiast ~7 MB), bo runner bez poprawnych wag kończy się
/// kodem 0 i zwraca same spacje.
pub(crate) fn check_weights(path: &Path) -> Result<()> {
    let mut f = std::fs::File::open(path).map_err(|e| {
        Error::Config(format!(
            "brak wag Phonemis: {}\nUstaw PHONEMIS_MODEL / Config::phonemis_weights (repo Phonemis: data/pl/phonemizer_pl.bin): {e}",
            path.display()
        ))
    })?;
    let mut head = [0u8; 64];
    let n = f.read(&mut head)?;
    if head[..n].starts_with(b"version https://git-lfs") {
        return Err(Error::Config(format!(
            "{} to wskaźnik Git LFS, a nie wagi modelu.\nPobierz je: git -C <repo Phonemis> lfs pull --include='data/pl/*'",
            path.display()
        )));
    }
    Ok(())
}

pub(crate) fn expand_home(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if s == "~" || s.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(s.trim_start_matches('~').trim_start_matches('/'));
        }
    }
    p.to_path_buf()
}

fn default_workers() -> usize {
    thread::available_parallelism().map_or(2, |n| n.get()).min(8)
}

/// Backend przez podproces: `phonemis_runner --lang pl --model <wagi> "tekst"`.
pub struct RunnerG2p {
    runner: PathBuf,
    weights: PathBuf,
    lang: String,
    timeout: Duration,
    workers: usize,
}

impl RunnerG2p {
    /// `weights == None` => `PHONEMIS_MODEL` albo `<repo>/data/pl/phonemizer_pl.bin` wyprowadzone z położenia runnera
    /// (`<repo>/build/phonemis_runner`).
    pub fn new(runner: impl AsRef<Path>, weights: Option<PathBuf>, workers: Option<usize>) -> Result<Self> {
        let runner = expand_home(runner.as_ref());
        let meta = std::fs::metadata(&runner).map_err(|e| Error::Config(format!("brak phonemis_runner: {}: {e}", runner.display())))?;
        if !meta.is_file() || !is_executable(&meta) {
            return Err(Error::Config(format!("{} nie jest plikiem wykonywalnym (chmod +x)", runner.display())));
        }
        let weights =
            weights.or_else(|| std::env::var_os("PHONEMIS_MODEL").filter(|v| !v.is_empty()).map(PathBuf::from)).unwrap_or_else(|| {
                let resolved = std::fs::canonicalize(&runner).unwrap_or_else(|_| runner.clone());
                let repo = resolved.parent().and_then(Path::parent).unwrap_or(Path::new("."));
                repo.join("data").join("pl").join("phonemizer_pl.bin")
            });
        let weights = expand_home(&weights);
        check_weights(&weights)?;
        Ok(Self {
            runner,
            weights,
            lang: "pl".into(),
            timeout: Duration::from_secs(60),
            workers: workers.filter(|&w| w > 0).unwrap_or_else(default_workers),
        })
    }

    /// Ścieżka wag, z których korzysta backend (przydatna w testach i diagnostyce).
    pub fn weights(&self) -> &Path {
        &self.weights
    }

    fn phonemize_one(&self, text: &str, cancels: &[&CancelToken]) -> Result<String> {
        let text = text.replace('\0', " ");
        let text = text.trim();
        if text.is_empty() {
            return Ok(String::new());
        }
        let cancelled = || cancels.iter().any(|c| c.is_cancelled());
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let mut child = Command::new(&self.runner)
            .arg("--lang")
            .arg(&self.lang)
            .arg("--model")
            .arg(&self.weights)
            .arg(text)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Runner(format!("uruchomienie phonemis_runner: {e}")))?;

        // Rury czytamy w osobnych wątkach (inaczej pełny bufor rury zablokowałby dziecko).
        let (tx_o, rx_o) = mpsc::channel();
        let (tx_e, rx_e) = mpsc::channel();
        let mut so = child.stdout.take().expect("stdout");
        let mut se = child.stderr.take().expect("stderr");
        thread::spawn(move || {
            let mut b = Vec::new();
            let _ = so.read_to_end(&mut b);
            let _ = tx_o.send(b);
        });
        thread::spawn(move || {
            let mut b = Vec::new();
            let _ = se.read_to_end(&mut b);
            let _ = tx_e.send(b);
        });

        let start = Instant::now();
        let mut wait = Duration::from_millis(1);
        let status = loop {
            match child.try_wait() {
                Ok(Some(st)) => break st,
                Ok(None) => {}
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(e.into());
                }
            }
            if cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Cancelled);
            }
            if start.elapsed() > self.timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Runner(format!("phonemis_runner: przekroczono limit czasu {:?}", self.timeout)));
            }
            thread::sleep(wait);
            wait = (wait * 2).min(Duration::from_millis(10));
        };
        // Potomkowie zabitego/zakończonego procesu mogą jeszcze trzymać rury — nie czekamy na nie w nieskończoność.
        let stdout = rx_o.recv_timeout(Duration::from_millis(500)).unwrap_or_default();
        let stderr = rx_e.recv_timeout(Duration::from_millis(500)).unwrap_or_default();
        let stdout = ANSI_RE.replace_all(&String::from_utf8_lossy(&stdout), "").into_owned();
        let stderr = ANSI_RE.replace_all(&String::from_utf8_lossy(&stderr), "").into_owned();

        if !status.success() {
            let msg = if stderr.trim().is_empty() { stdout.trim() } else { stderr.trim() };
            return Err(match status.code() {
                Some(code) => Error::Runner(format!("phonemis_runner zakończył się kodem {code}: {msg}")),
                None => Error::Runner(format!("phonemis_runner przerwany sygnałem: {msg}")),
            });
        }
        for line in stdout.lines() {
            if let Some(rest) = line.strip_prefix("Output: ") {
                return Ok(rest.trim().to_string());
            }
        }
        Err(Error::Runner(format!("nie znaleziono linii 'Output:' w wyjściu runnera: {stdout:?}")))
    }
}

#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_: &std::fs::Metadata) -> bool {
    true
}

impl G2p for RunnerG2p {
    fn phonemize(&self, text: &str, cancel: &CancelToken) -> Result<String> {
        self.phonemize_one(text, &[cancel])
    }

    /// Równolegle (model ładuje się przy każdym wywołaniu runnera). Pierwszy błąd przerywa pozostałe podprocesy.
    fn phonemize_many(&self, texts: &[String], cancel: &CancelToken) -> Result<Vec<String>> {
        let n = texts.len();
        let results: Mutex<Vec<String>> = Mutex::new(vec![String::new(); n]);
        let first_err: Mutex<Option<Error>> = Mutex::new(None);
        let stop = CancelToken::new();
        let next = AtomicUsize::new(0);
        thread::scope(|s| {
            for _ in 0..self.workers.min(n) {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    if i >= n || stop.is_cancelled() || cancel.is_cancelled() {
                        break;
                    }
                    match self.phonemize_one(&texts[i], &[cancel, &stop]) {
                        Ok(r) => results.lock().unwrap()[i] = r,
                        Err(e) => {
                            let mut slot = first_err.lock().unwrap();
                            if slot.is_none() {
                                *slot = Some(e);
                            }
                            stop.cancel();
                            break;
                        }
                    }
                });
            }
        });
        cancel.check()?;
        if let Some(e) = first_err.into_inner().unwrap() {
            return Err(e);
        }
        Ok(results.into_inner().unwrap())
    }
}
