//! Wspólne fixtures testów integracyjnych: fałszywy serwer HF (tiny_http), fałszywy `phonemis_runner`, katalog modelu.
#![allow(dead_code)]

use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use plkokoro::{Config, REPO_ID, REVISION};
use tiny_http::{Header, Request, Response, Server};

/// Ścieżka do `libonnxruntime.so` albo `None` (testy wymagające sesji ONNX się wtedy pomijają).
pub fn ort_lib() -> Option<PathBuf> {
    std::env::var_os("ORT_LIBRARY_PATH").filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Pomija test (wypisuje `SKIP`), gdy nie ma `ORT_LIBRARY_PATH`. Libtest nie ma prawdziwego „skip", więc sprawdzaj
/// `cargo test -- --nocapture | grep SKIP`.
#[macro_export]
macro_rules! require_ort {
    () => {
        match $crate::common::ort_lib() {
            Some(p) => p,
            None => {
                eprintln!("SKIP: ustaw ORT_LIBRARY_PATH na libonnxruntime.so (>= 1.21), by uruchomić test z prawdziwym ORT");
                return;
            }
        }
    };
}

pub const FAKE_VOCAB_CHARS: &str = "abcdefghijklmnopqrstuvwxyzʒ ,.!?:;—…\"'";

pub fn vocab_id(c: char) -> i64 {
    FAKE_VOCAB_CHARS.chars().position(|x| x == c).map_or(-1, |i| i as i64 + 1)
}

/// Deterministyczna macierz stylu: wiersz `r`, kolumna `j`.
pub fn style_val(r: usize, j: usize) -> f32 {
    (r % 7) as f32 * 0.25 + (j % 5) as f32 * 0.01
}

pub fn style_bytes() -> Vec<u8> {
    let mut out = Vec::with_capacity(plkokoro::STYLE_ROWS * 256 * 4);
    for r in 0..plkokoro::STYLE_ROWS {
        for j in 0..256 {
            out.extend_from_slice(&style_val(r, j).to_le_bytes());
        }
    }
    out
}

pub fn vocab_json() -> String {
    let mut m = serde_json::Map::new();
    for (i, c) in FAKE_VOCAB_CHARS.chars().enumerate() {
        m.insert(c.to_string(), serde_json::json!(i as i64 + 1));
    }
    serde_json::json!({ "vocab": m, "other": 1 }).to_string()
}

pub fn catalog_json(voice_path: &str) -> String {
    serde_json::json!({
        "runtime": {"tokenEncoding": {"vocabularyField": "vocab"}},
        "languages": [
            {"id": "en", "defaultVoiceId": "x", "voices": []},
            {"id": "pl", "defaultVoiceId": "v2", "voices": [
                {"id": "v1", "modelId": "m0", "artifact": {"path": "voices/other.bin"}},
                {"id": "v2", "modelId": "m1", "artifact": {"path": voice_path}}
            ]}
        ],
        "models": [{"id": "m1", "tokenizerId": "t1", "artifact": {"path": "onnx/model.onnx"}}],
        "tokenizers": [{"id": "t1", "artifact": {"path": "cfg.json"}}]
    })
    .to_string()
}

pub fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../testdata").join(name)
}

pub fn read_testdata(name: &str) -> Vec<u8> {
    std::fs::read(testdata(name)).unwrap_or_else(|e| panic!("brak testdata/{name}: {e}"))
}

/// Serwer HTTP w tle. Zatrzymuje się przy `Drop`.
pub struct TestServer {
    server: Arc<Server>,
    handle: Option<JoinHandle<()>>,
    pub requests: Arc<AtomicUsize>,
    pub url: String,
}

impl TestServer {
    pub fn start(handler: impl Fn(Request) + Send + Sync + 'static) -> Self {
        let server = Arc::new(Server::http("127.0.0.1:0").expect("bind"));
        let url = format!("http://{}", server.server_addr().to_ip().expect("ip"));
        let requests = Arc::new(AtomicUsize::new(0));
        let (srv, count) = (server.clone(), requests.clone());
        let handle = std::thread::spawn(move || {
            for req in srv.incoming_requests() {
                count.fetch_add(1, Ordering::SeqCst);
                handler(req);
            }
        });
        Self { server, handle: Some(handle), requests, url }
    }

    pub fn count(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

// Zachowanie fałszywego runnera zależy od TREŚCI tekstu (nie od zmiennych środowiskowych — testy idą równolegle
// w jednym procesie): BOOM = błąd (kod 1), HANG = wisi 30 s (do testów anulowania), SLOW = opóźnienie 0,3 s.

/// Udaje HF: `/<repo>/resolve/<rewizja>/<ścieżka>`; model jest serwowany przez przekierowanie na „CDN", jak prawdziwy HF.
pub fn fake_hf_with(files: Vec<(&'static str, Vec<u8>)>) -> TestServer {
    let prefix = format!("/{REPO_ID}/resolve/{REVISION}/");
    TestServer::start(move |req: Request| {
        let url = req.url().to_string();
        if let Some(rel) = url.strip_prefix("/cdn/") {
            return match files.iter().find(|(n, _)| *n == rel) {
                Some((_, data)) => drop(req.respond(Response::from_data(data.clone()))),
                None => drop(req.respond(Response::empty(404))),
            };
        }
        let Some(rel) = url.strip_prefix(&prefix) else { return drop(req.respond(Response::empty(404))) };
        match files.iter().find(|(n, _)| *n == rel) {
            None => drop(req.respond(Response::empty(404))),
            Some(_) if rel == "onnx/model.onnx" => {
                let loc = Header::from_bytes("Location", format!("/cdn/{rel}")).unwrap();
                drop(req.respond(Response::empty(302).with_header(loc)))
            }
            Some((_, data)) => drop(req.respond(Response::new(200.into(), vec![], Cursor::new(data.clone()), Some(data.len()), None))),
        }
    })
}

pub fn default_files() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("catalog.json", catalog_json("voices/pl.bin").into_bytes()),
        ("cfg.json", vocab_json().into_bytes()),
        ("voices/pl.bin", style_bytes()),
        ("onnx/model.onnx", read_testdata("tiny_kokoro.onnx")),
    ]
}

pub fn fake_hf() -> TestServer {
    fake_hf_with(default_files())
}

const FAKE_RUNNER: &str = r#"#!/usr/bin/env bash
model=""; text="${@: -1}"
for ((i=1;i<=$#;i++)); do [ "${!i}" = "--model" ] && { j=$((i+1)); model="${!j}"; }; done
[ -z "$model" ] && { printf '\n\033[1;32mPhonemization Result:\033[0m\n\033[1;36mOutput: \033[0m    .\n'; exit 0; }
case "$text" in *BOOM*) printf '\033[1;31mError:\033[0m zepsuty model\n' >&2; exit 1;; *HANG*) exec sleep 30;; esac
case "$text" in *SLOW*) sleep 0.3;; esac
printf '\n\033[1;32mPhonemization Result:\033[0m\n\033[1;34mInput:  \033[0m%s\n\033[1;36mOutput: \033[0m%s\n\033[1;33mTime:   \033[0m0.02 ms\n\n' "$text" "$(printf '%s' "$text" | tr 'A-Z' 'a-z')"
"#;

/// Układ jak po `pk-phonemis-build`: `<tmp>/repo/build/phonemis_runner` + `data/pl/phonemizer_pl.bin`.
/// Zwraca (runner, wagi); katalog zostaje w `dir`.
pub fn fake_runner(dir: &Path) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let root = dir.join("repo");
    let runner = root.join("build").join("phonemis_runner");
    let weights = root.join("data").join("pl").join("phonemizer_pl.bin");
    std::fs::create_dir_all(runner.parent().unwrap()).unwrap();
    std::fs::create_dir_all(weights.parent().unwrap()).unwrap();
    std::fs::write(&runner, FAKE_RUNNER).unwrap();
    std::fs::set_permissions(&runner, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(&weights, vec![7u8; 4096]).unwrap();
    (runner, weights)
}

/// Konfiguracja z fałszywym HF, runnerem i katalogiem tymczasowym `tmp`.
pub fn base_cfg(tmp: &Path, hf_url: &str, ort: &Path) -> Config {
    let (runner, _) = fake_runner(tmp);
    Config {
        model_dir: Some(tmp.join("models")),
        hf_endpoint: Some(hf_url.to_string()),
        phonemis_runner: Some(runner),
        ort_library: Some(ort.to_path_buf()),
        ..Default::default()
    }
}

/// Logger zbierający ostrzeżenia (do sprawdzania komunikatu „raz na znak"). Globalny dla procesu testowego,
/// więc asercje filtruj po unikalnym znaku.
pub mod logs {
    use std::sync::{Mutex, Once};

    static MESSAGES: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static INIT: Once = Once::new();
    struct Collect;
    impl log::Log for Collect {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= log::Level::Warn
        }
        fn log(&self, r: &log::Record) {
            if self.enabled(r.metadata()) {
                MESSAGES.lock().unwrap().push(r.args().to_string());
            }
        }
        fn flush(&self) {}
    }

    pub fn init() {
        INIT.call_once(|| {
            let _ = log::set_logger(&Collect).map(|()| log::set_max_level(log::LevelFilter::Warn));
        });
    }

    pub fn count_containing(needle: &str) -> usize {
        MESSAGES.lock().unwrap().iter().filter(|m| m.contains(needle)).count()
    }
}

/// Surowy serwer TCP: na każde połączenie czyta nagłówki żądania, zapisuje `response` i ZAMYKA połączenie.
/// (tiny_http przy niezgodnym Content-Length zostawia gniazdo otwarte, więc nie nadaje się do testu ucięcia.)
pub fn raw_server(response: Vec<u8>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 4096];
            let mut seen = Vec::new();
            while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                match s.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => seen.extend_from_slice(&buf[..n]),
                }
            }
            let _ = s.write_all(&response);
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    });
    url
}
