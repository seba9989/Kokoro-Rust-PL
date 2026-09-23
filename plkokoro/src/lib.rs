//! Polski TTS jako biblioteka: **Kokoro** (`Shusek00/kokoro-kmp-models`, ONNX) + frontend tekstowy **Phonemis**
//! (<https://github.com/IgorSwat/Phonemis>). Port wersji Pythona (`pl_kokoro_tts.py`) i Go (`plkokoro`).
//!
//! Cztery operacje:
//!
//! ```no_run
//! use plkokoro::{load_model, Config, SynthOptions, TextOptions};
//!
//! # fn main() -> plkokoro::Result<()> {
//! let model = load_model(Config {
//!     phonemis_runner: Some("/…/phonemis/build/phonemis_runner".into()),   // albo PHONEMIS_RUNNER
//!     ..Default::default()
//! })?;                                                                       // 1. ładowanie
//! let ipa = model.text_to_ipa("Lubię [Kokoro](/kɔkˈɔrɔ/), ok. 5 zł.", &TextOptions::default())?; // 2. tekst -> IPA
//! let wav = model.synthesize(&ipa, &SynthOptions::default())?;               // 3. IPA -> Vec<f32>
//! model.unload()?;                                                           // 4. zwolnienie pamięci
//! plkokoro::write_wav("out.wav", &wav)?;                                     // mono, SAMPLE_RATE Hz
//! # Ok(()) }
//! ```
//!
//! Format IPA zwracanego przez `text_to_ipa` (i przyjmowanego przez `synthesize`): jedno zdanie na linię, pusta linia
//! = koniec akapitu. Zwykły jednowierszowy napis IPA też jest poprawny — zostanie pocięty na porcje (limit kontekstu
//! Kokoro = 510 tokenów).
//!
//! Wymagania w czasie działania: `libonnxruntime.so` >= 1.21 (`Config::ort_library` / `ORT_LIBRARY_PATH`) oraz
//! `phonemis_runner` z wagami języka fonemizera (np. `phonemizer_pl.bin`). Zmienne środowiskowe: `KOKORO_MODEL_DIR`,
//! `KOKORO_OFFLINE`, `KOKORO_LANG`, `KOKORO_VOICE`, `HF_ENDPOINT`, `HF_TOKEN`, `PHONEMIS_RUNNER`, `PHONEMIS_MODEL`,
//! `PHONEMIS_LANG`, `ORT_LIBRARY_PATH`.
//!
//! Domyślnie wszystko jest polskie. Język mówcy (`Config::lang`), głos (`Config::voice`, lista: `list_voices`)
//! i język fonemizera (`Config::phonemis_lang`) wybiera się niezależnie — np. polski tekst czytany głosem `af_heart`.
//!
//! Logi idą przez fasadę `log` (`[model]`, `[normalize]`, `[phonemis]`, `[uwaga]`); bez skonfigurowanego loggera nic
//! nie jest wypisywane.

mod cancel;
mod engine;
mod error;
mod g2p;
mod normalize;
mod overrides;
mod store;
mod text;
mod trim;
mod wav;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, RwLock, RwLockReadGuard};
use std::time::Duration;

use log::{debug, warn};
use regex::Regex;

pub use cancel::CancelToken;
pub use error::{Error, Result};
pub use g2p::{phonemis_weights_file, G2p, RunnerG2p, PHONEMIS_LANGS};
pub use normalize::normalize_pl;
#[allow(deprecated)]
pub use store::LANG_ID;
pub use store::{VoiceInfo, DEFAULT_LANG, REPO_ID, REVISION, SAMPLE_RATE, STYLE_ROWS};
pub use wav::{encode_wav, write_wav};

use engine::{Engine, OrtEngine};
use overrides::{extract_overrides, sentence_ipa, show_overrides, text_pieces};
use store::{Store, STYLE_DIM};
use text::{collapse_spaces, pack_ipa, split_sentences_lang};

#[doc(hidden)]
pub mod __internal {
    //! Tylko dla testów zgodności; nie jest częścią stabilnego API.
    pub use crate::overrides::{extract_overrides, sentence_ipa};
    pub use crate::text::{pack_ipa, split_sentences};
}

/// Konfiguracja `load_model` i `download_model`. Wszystkie pola są opcjonalne; puste wartości uzupełniają zmienne
/// środowiskowe.
#[derive(Default)]
pub struct Config {
    /// Lokalna kopia Kokoro (domyślnie `KOKORO_MODEL_DIR` albo `<katalog cache>/kokoro-pl/kokoro-kmp-models`).
    pub model_dir: Option<PathBuf>,
    /// `true` = nigdy nie łącz się z HF (`KOKORO_OFFLINE=1` też włącza).
    pub offline: bool,
    /// Serwer z modelem (domyślnie `HF_ENDPOINT` albo `https://huggingface.co`).
    pub hf_endpoint: Option<String>,
    /// Język mówcy — id języka z `catalog.json`, np. `pl`, `de`, `en-us` (albo `KOKORO_LANG`; domyślnie `DEFAULT_LANG`).
    /// Wyznacza głos, model ONNX i tokenizer.
    pub lang: Option<String>,
    /// Głos w języku mówcy, np. `pm_mateusz`, `af_heart` (albo `KOKORO_VOICE`; domyślnie głos domyślny języka).
    /// Lista: `list_voices`.
    pub voice: Option<String>,
    /// Język fonemizera Phonemis (`PHONEMIS_LANGS`; albo `PHONEMIS_LANG`). Domyślnie język frontendu tekstowego
    /// z katalogu dla języka mówcy (np. `pt-br` -> `pt`); dla ja/zh nie ma frontendu — wtedy `text_to_ipa` wymaga
    /// własnego `g2p`, a `synthesize` przyjmuje gotowe IPA.
    pub phonemis_lang: Option<String>,
    /// Ścieżka do `phonemis_runner` (albo `PHONEMIS_RUNNER`).
    pub phonemis_runner: Option<PathBuf>,
    /// Wagi Phonemis dla języka fonemizera, np. `phonemizer_pl.bin` (albo `PHONEMIS_MODEL`; domyślnie
    /// `<repo>/data/<język>/phonemizer_<język>.bin` wyprowadzane z położenia runnera).
    pub phonemis_weights: Option<PathBuf>,
    /// Własny backend fonemizacji zamiast runnera; `Model` zamknie go w `unload`.
    pub g2p: Option<Box<dyn G2p>>,
    /// Równoległość fonemizacji (domyślnie `min(8, liczba CPU)`).
    pub workers: Option<usize>,
    /// Ścieżka do `libonnxruntime.so` (albo `ORT_LIBRARY_PATH`).
    pub ort_library: Option<PathBuf>,
    /// Providery ORT, np. `["CUDAExecutionProvider"]`; domyślnie CPU. Poza CPU wymagają cechy Cargo (`cuda`,
    /// `tensorrt`, `rocm`, `migraphx`, `coreml`, `directml`, `openvino`) oraz biblioteki ORT z danym providerem.
    pub providers: Vec<String>,
    /// `true` = tylko G2P (`text_to_ipa`), bez pobierania i ładowania Kokoro.
    pub skip_kokoro: bool,
    /// Znacznik anulowania współdzielony z wywołującym (patrz `Model::cancel_token`).
    pub cancel: Option<CancelToken>,
}

/// Opcje `Model::text_to_ipa`.
#[derive(Clone, Debug, Default)]
pub struct TextOptions {
    /// Wyłącz normalizację uzupełniającą (zł, %, skróty…).
    pub no_normalize: bool,
}

/// Opcje `Model::synthesize`. Zacznij od `SynthOptions::default()` i zmieniaj wybrane pola:
/// `SynthOptions { speed: 1.2, ..Default::default() }`. Pauza równa zeru oznacza brak pauzy.
#[derive(Clone, Debug)]
pub struct SynthOptions {
    /// Tempo mowy; musi być > 0 (domyślnie 1.0).
    pub speed: f32,
    /// Limit fonemów na porcję (domyślnie 300, twardo <= 510).
    pub max_phonemes: usize,
    /// Cisza po zdaniu (domyślnie 200 ms).
    pub sentence_pause: Duration,
    /// Cisza po ostatnim zdaniu akapitu (domyślnie 450 ms).
    pub paragraph_pause: Duration,
    /// Cisza między porcjami jednego zdania (domyślnie 80 ms).
    pub clause_pause: Duration,
}

impl Default for SynthOptions {
    fn default() -> Self {
        Self {
            speed: 1.0,
            max_phonemes: 300,
            sentence_pause: Duration::from_millis(200),
            paragraph_pause: Duration::from_millis(450),
            clause_pause: Duration::from_millis(80),
        }
    }
}

const MAX_CACHE_ENTRIES: usize = 20_000;

/// Wynik ładowania Kokoro: słownik, macierz stylu, silnik.
type Loaded = (HashMap<char, i64>, Vec<f32>, Box<dyn Engine>);

struct Inner {
    g2p: Option<Box<dyn G2p>>, // None: język bez frontendu (ja, zh) — tylko synteza z gotowego IPA
    engine: Option<Box<dyn Engine>>,
    vocab: HashMap<char, i64>,
    style: Vec<f32>,
    cache: Mutex<HashMap<String, String>>, // kawałek tekstu -> surowe IPA
    warned: Mutex<HashSet<char>>,
}

/// Co wybrano przy ładowaniu (zostaje dostępne także po `unload`).
#[derive(Clone, Debug, Default)]
pub(crate) struct Selection {
    lang: Option<String>,
    voice: Option<String>,
    g2p_lang: Option<String>,
}

/// Załadowany model (backend Phonemis + sesja Kokoro). Bezpieczny współbieżnie (`Send + Sync`): `unload` czeka na
/// trwające wywołania.
pub struct Model {
    state: RwLock<Option<Inner>>,
    cancel: CancelToken,
    dir: PathBuf,
    sel: Selection,
}

/// `Some(v)` z pola konfiguracji albo niepustej zmiennej środowiskowej.
fn cfg_or_env(v: &Option<String>, env: &str) -> Option<String> {
    v.clone().filter(|s| !s.is_empty()).or_else(|| std::env::var(env).ok().filter(|s| !s.is_empty()))
}

fn speaker_lang(cfg: &Config) -> Option<String> {
    cfg_or_env(&cfg.lang, "KOKORO_LANG")
}

fn speaker_voice(cfg: &Config) -> Option<String> {
    cfg_or_env(&cfg.voice, "KOKORO_VOICE")
}

/// Pobiera pliki Kokoro wybranego języka i głosu (`Config::lang`, `Config::voice`) do lokalnego katalogu i zwraca ich
/// ścieżki. Nic nie ładuje do pamięci.
pub fn download_model(cfg: &Config) -> Result<Vec<PathBuf>> {
    let store = Store::new(cfg);
    let cancel = cfg.cancel.clone().unwrap_or_default();
    let cat = store.load_catalog(&cancel)?;
    let r = cat.resolve(speaker_lang(cfg).as_deref().unwrap_or(DEFAULT_LANG), speaker_voice(cfg).as_deref())?;
    let mut paths = vec![store.base().join("catalog.json")];
    for rel in [&r.tokenizer, &r.voice, &r.model] {
        paths.push(store.get(rel, &cancel)?);
    }
    Ok(paths)
}

/// Wszystkie głosy z `catalog.json` (wszystkie języki). Czyta tylko katalog — przy braku lokalnej kopii pobiera go
/// (kilkadziesiąt KB), chyba że `offline`.
pub fn list_voices(cfg: &Config) -> Result<Vec<VoiceInfo>> {
    let store = Store::new(cfg);
    let cancel = cfg.cancel.clone().unwrap_or_default();
    Ok(store.load_catalog(&cancel)?.voices())
}

/// Ładuje model: backend Phonemis + Kokoro (sesja ONNX, słownik, styl głosu) dla wybranego języka mówcy, głosu
/// i języka fonemizera. Brakujące pliki modelu są pobierane raz do `Config::model_dir`, potem sieć nie jest używana.
/// Najpierw czytany jest mały `catalog.json` (wybór głosu), potem powstaje backend G2P — błąd konfiguracji wychodzi
/// przed pobieraniem kilkuset MB. Przy `skip_kokoro` bez `Config::lang` katalog nie jest czytany wcale.
pub fn load_model(mut cfg: Config) -> Result<Model> {
    let cancel = cfg.cancel.clone().unwrap_or_default();
    let store = Store::new(&cfg);
    let dir = store.root().to_path_buf();
    let lang = speaker_lang(&cfg);

    let resolved = if cfg.skip_kokoro && lang.is_none() {
        None
    } else {
        let cat = store.load_catalog(&cancel)?;
        Some(cat.resolve(lang.as_deref().unwrap_or(DEFAULT_LANG), speaker_voice(&cfg).as_deref())?)
    };
    let g2p_lang = match (cfg_or_env(&cfg.phonemis_lang, "PHONEMIS_LANG"), &resolved) {
        (Some(l), _) => Some(l),
        (None, Some(r)) => r.g2p_lang.clone(),
        (None, None) => Some(DEFAULT_LANG.to_string()),
    };
    let sel = Selection {
        lang: resolved.as_ref().map(|r| r.lang.clone()),
        voice: resolved.as_ref().map(|r| r.voice_id.clone()),
        g2p_lang: g2p_lang.clone(),
    };

    let g2p: Option<Box<dyn G2p>> = match (cfg.g2p.take(), &g2p_lang) {
        (Some(g), _) => Some(g),
        (None, None) => None,
        (None, Some(gl)) => {
            let runner = cfg
                .phonemis_runner
                .clone()
                .or_else(|| std::env::var_os("PHONEMIS_RUNNER").filter(|v| !v.is_empty()).map(PathBuf::from))
                .ok_or_else(|| {
                    Error::Config(
                        "nie podano Phonemis: ustaw Config::phonemis_runner (albo PHONEMIS_RUNNER) na ścieżkę do phonemis_runner \
                         albo podaj własny Config::g2p"
                            .into(),
                    )
                })?;
            Some(Box::new(RunnerG2p::new(runner, gl, cfg.phonemis_weights.clone(), cfg.workers)?))
        }
    };
    let Some(r) = resolved.filter(|_| !cfg.skip_kokoro) else {
        return Ok(Model::from_parts(g2p, None, HashMap::new(), Vec::new(), cancel, dir, sel));
    };
    let loaded = (|| -> Result<Loaded> {
        let vocab = store.load_vocab(&r, &cancel)?;
        let style = store.load_style(&r, &cancel)?;
        let model_path = store.get(&r.model, &cancel)?;
        let engine = OrtEngine::new(cfg.ort_library.as_deref(), &model_path, &cfg.providers)?;
        Ok((vocab, style, Box::new(engine)))
    })();
    match loaded {
        Ok((vocab, style, engine)) => Ok(Model::from_parts(g2p, Some(engine), vocab, style, cancel, dir, sel)),
        Err(e) => {
            if let Some(g) = g2p {
                let _ = g.close();
            }
            Err(e)
        }
    }
}

static PARA_SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n\s*\n").unwrap());

struct Chunk {
    ipa: String,
    pause: Duration,
}

impl Model {
    pub(crate) fn from_parts(
        g2p: Option<Box<dyn G2p>>,
        engine: Option<Box<dyn Engine>>,
        vocab: HashMap<char, i64>,
        style: Vec<f32>,
        cancel: CancelToken,
        dir: PathBuf,
        sel: Selection,
    ) -> Model {
        let inner = Inner { g2p, engine, vocab, style, cache: Mutex::new(HashMap::new()), warned: Mutex::new(HashSet::new()) };
        Model { state: RwLock::new(Some(inner)), cancel, dir, sel }
    }

    fn read(&self) -> RwLockReadGuard<'_, Option<Inner>> {
        self.state.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Czy model jest załadowany (`false` po `unload`).
    pub fn is_loaded(&self) -> bool {
        self.read().is_some()
    }

    /// Katalog lokalnej kopii modelu Kokoro (bez podkatalogu rewizji).
    pub fn model_dir(&self) -> &Path {
        &self.dir
    }

    /// Język mówcy (id z katalogu); `None`, gdy załadowano z `skip_kokoro` bez `Config::lang`.
    pub fn lang(&self) -> Option<&str> {
        self.sel.lang.as_deref()
    }

    /// Id wybranego głosu; `None` jak w `lang`.
    pub fn voice(&self) -> Option<&str> {
        self.sel.voice.as_deref()
    }

    /// Język fonemizera (Phonemis); `None` = brak frontendu tekstowego (ja, zh bez własnego `phonemis_lang`).
    pub fn phonemis_lang(&self) -> Option<&str> {
        self.sel.g2p_lang.as_deref()
    }

    /// Znacznik anulowania tego modelu: `cancel()` przerywa trwającą fonemizację i syntezę (patrz `CancelToken`).
    pub fn cancel_token(&self) -> CancelToken {
        self.cancel.clone()
    }

    /// Zamienia tekst na IPA. Obsługuje ręczne fonemy `[tekst](/ipa/)` oraz — dla języka fonemizera `pl` —
    /// normalizację uzupełniającą. Zwraca jedno zdanie na linię, pusta linia = granica akapitu; puste wejście daje `""`.
    /// IPA nie jest tu filtrowane słownikiem Kokoro — robi to `synthesize`.
    pub fn text_to_ipa(&self, text: &str, opts: &TextOptions) -> Result<String> {
        let guard = self.read();
        let inner = guard.as_ref().ok_or(Error::Unloaded)?;
        let g2p = inner.g2p.as_deref().ok_or_else(|| {
            Error::Config(format!(
                "język mówcy {:?} nie ma frontendu tekstowego w Phonemis — podaj gotowe IPA do synthesize (CLI: --ipa), \
                 ustaw Config::phonemis_lang (--phonemis-lang) albo własny Config::g2p",
                self.sel.lang.as_deref().unwrap_or("?")
            ))
        })?;
        let polish = self.sel.g2p_lang.as_deref() == Some("pl"); // normalizacja i podział zdań jak w wersji Pythona
        let (text, overrides) = extract_overrides(text);
        let text = if opts.no_normalize || !polish { text } else { normalize_pl(&text) };
        debug!("[normalize] {}", show_overrides(&text, &overrides));

        let paragraphs = split_sentences_lang(&text, polish);
        let pieces: Vec<String> = paragraphs.iter().flatten().flat_map(|s| text_pieces(s)).collect();
        inner.prefetch(g2p, &pieces, &self.cancel)?;

        let cache = inner.cache.lock().unwrap_or_else(|e| e.into_inner());
        let ipa_of = |piece: &str| cache.get(piece).cloned().unwrap_or_default();
        let mut out: Vec<String> = Vec::new();
        for para in &paragraphs {
            let mut lines = Vec::new();
            for s in para {
                let ipa = sentence_ipa(s, &overrides, &ipa_of);
                debug!("[phonemis] {} -> {ipa}", show_overrides(s, &overrides));
                if !ipa.is_empty() {
                    lines.push(ipa);
                }
            }
            if !lines.is_empty() {
                out.push(lines.join("\n"));
            }
        }
        Ok(out.join("\n\n"))
    }

    /// Zamienia IPA na audio na załadowanym modelu: mono `f32`, `SAMPLE_RATE` Hz. Linie = zdania, pusta linia =
    /// akapit; zbyt długie zdania są cięte na porcje `<= max_phonemes` (twardo 510) po interpunkcji, potem po
    /// spacjach. Znaki spoza słownika Kokoro są pomijane z ostrzeżeniem. Anulowanie działa między porcjami.
    pub fn synthesize(&self, ipa: &str, opts: &SynthOptions) -> Result<Vec<f32>> {
        let guard = self.read();
        let inner = guard.as_ref().ok_or(Error::Unloaded)?;
        let engine = inner.engine.as_ref().ok_or(Error::NoKokoro)?;
        if opts.speed.is_nan() || opts.speed <= 0.0 {
            return Err(Error::InvalidOption(format!("speed musi być > 0, dostałem {}", opts.speed)));
        }
        let limit = opts.max_phonemes.clamp(1, STYLE_ROWS);

        let mut chunks: Vec<Chunk> = Vec::new();
        for para in PARA_SPLIT_RE.split(ipa) {
            let lines: Vec<String> = para.split('\n').map(|l| inner.filter_ipa(l)).filter(|l| !l.is_empty()).collect();
            for (li, line) in lines.iter().enumerate() {
                let pieces = pack_ipa(line, limit);
                for (pi, piece) in pieces.iter().enumerate() {
                    let pause = if pi + 1 < pieces.len() {
                        opts.clause_pause
                    } else if li + 1 == lines.len() {
                        opts.paragraph_pause
                    } else {
                        opts.sentence_pause
                    };
                    chunks.push(Chunk { ipa: piece.clone(), pause });
                }
            }
        }
        if chunks.is_empty() {
            return Err(Error::NoPhonemes);
        }

        let mut wav: Vec<f32> = Vec::new();
        for (i, ch) in chunks.iter().enumerate() {
            self.cancel.check()?;
            let chars: Vec<char> = ch.ipa.chars().collect();
            let n = chars.len();
            let mut ids: Vec<i64> = Vec::with_capacity(n + 2);
            ids.push(0); // BOS
            ids.extend(chars.iter().map(|c| inner.vocab[c]));
            ids.push(0); // EOS
            let row = &inner.style[(n - 1) * STYLE_DIM..n * STYLE_DIM];
            wav.extend(engine.run(&ids, row, opts.speed)?);
            if !ch.pause.is_zero() && i + 1 < chunks.len() {
                wav.resize(wav.len() + (f64::from(SAMPLE_RATE) * ch.pause.as_secs_f64()) as usize, 0.0);
            }
        }
        Ok(wav)
    }

    /// Zwalnia model z pamięci (sesja ONNX, styl, słownik, backend Phonemis). Idempotentne; czeka na trwające
    /// wywołania. Po `unload` operacje zwracają `Error::Unloaded`. Uwaga: biblioteka `libonnxruntime.so` pozostaje
    /// zmapowana do końca procesu (środowisko ORT w crate `ort` jest globalne).
    pub fn unload(&self) -> Result<()> {
        let taken = self.state.write().unwrap_or_else(|e| e.into_inner()).take(); // blokada wyłączna czeka na czytelników
        let Some(inner) = taken else { return Ok(()) };
        let Inner { g2p, engine, .. } = inner;
        drop(engine); // niszczy sesję ONNX
        let res = g2p.as_ref().map_or(Ok(()), |g| g.close());
        drop(g2p);
        trim::trim_native();
        res
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        let _ = self.unload();
    }
}

impl Inner {
    /// Fonemizuje jeszcze nieznane kawałki tekstu (równolegle, jeśli backend to umie).
    fn prefetch(&self, g2p: &dyn G2p, pieces: &[String], cancel: &CancelToken) -> Result<()> {
        let todo: Vec<String> = {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            let mut seen = HashSet::new();
            pieces.iter().filter(|p| !cache.contains_key(*p) && seen.insert(p.as_str())).cloned().collect()
        };
        if todo.is_empty() {
            return Ok(());
        }
        let raw = g2p.phonemize_many(&todo, cancel)?;
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() + todo.len() > MAX_CACHE_ENTRIES {
            cache.clear(); // prosta ochrona przed nieograniczonym wzrostem w długo żyjącym procesie
        }
        for (piece, ipa) in todo.into_iter().zip(raw) {
            cache.insert(piece, ipa);
        }
        Ok(())
    }

    /// Zostawia tylko znaki ze słownika Kokoro (reszta: ostrzeżenie raz na znak).
    fn filter_ipa(&self, ipa: &str) -> String {
        let ipa = collapse_spaces(ipa); // najpierw ujednolić białe znaki, potem filtr po słowniku
        let mut kept = String::with_capacity(ipa.len());
        let mut dropped: Vec<char> = Vec::new();
        {
            let mut warned = self.warned.lock().unwrap_or_else(|e| e.into_inner());
            for c in ipa.chars() {
                if self.vocab.contains_key(&c) {
                    kept.push(c);
                } else if warned.insert(c) {
                    dropped.push(c);
                }
            }
        }
        if !dropped.is_empty() {
            warn!("[uwaga] znaki spoza słownika Kokoro zostaną pominięte: {}", dropped.iter().collect::<String>());
        }
        collapse_spaces(&kept)
    }
}

#[cfg(test)]
mod tests {
    //! Logika `Model` na atrapie silnika i backendu — bez ONNX Runtime i bez sieci.

    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use super::*;

    /// Zwraca `100 * (liczba_tokenów)` próbek o wartości 1.0 i zapamiętuje, co dostał.
    struct FakeEngine {
        calls: Mutex<Vec<(Vec<i64>, usize, f32)>>, // (ids, indeks wiersza stylu = pierwszy element stylu, speed)
    }

    impl Engine for FakeEngine {
        fn run(&self, ids: &[i64], style: &[f32], speed: f32) -> Result<Vec<f32>> {
            assert_eq!(style.len(), STYLE_DIM);
            self.calls.lock().unwrap().push((ids.to_vec(), style[0] as usize, speed));
            Ok(vec![1.0; ids.len() * 100])
        }
    }

    struct FakeG2p(Arc<AtomicBool>);

    impl G2p for FakeG2p {
        fn phonemize(&self, text: &str, _: &CancelToken) -> Result<String> {
            Ok(text.to_lowercase())
        }
        fn close(&self) -> Result<()> {
            self.0.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    const CHARS: &str = "abcdefghijklmnopqrstuvwxyzɛ ,.!?";

    /// Model z atrapami; `style[wiersz*STYLE_DIM]` = numer wiersza (żeby widzieć, który wiersz wybrano).
    fn fake_model(with_engine: bool) -> (Model, Arc<FakeEngine>, Arc<AtomicBool>) {
        let engine = Arc::new(FakeEngine { calls: Mutex::new(Vec::new()) });
        let closed = Arc::new(AtomicBool::new(false));
        let vocab: HashMap<char, i64> = CHARS.chars().enumerate().map(|(i, c)| (c, i as i64 + 1)).collect();
        let mut style = vec![0.0f32; STYLE_ROWS * STYLE_DIM];
        for r in 0..STYLE_ROWS {
            style[r * STYLE_DIM] = r as f32;
        }
        struct Shared(Arc<FakeEngine>);
        impl Engine for Shared {
            fn run(&self, ids: &[i64], style: &[f32], speed: f32) -> Result<Vec<f32>> {
                self.0.run(ids, style, speed)
            }
        }
        let eng: Option<Box<dyn Engine>> = with_engine.then(|| Box::new(Shared(engine.clone())) as Box<dyn Engine>);
        let sel = Selection { lang: Some("pl".into()), voice: Some("v".into()), g2p_lang: Some("pl".into()) };
        let m = Model::from_parts(Some(Box::new(FakeG2p(closed.clone()))), eng, vocab, style, CancelToken::new(), PathBuf::from("/x"), sel);
        (m, engine, closed)
    }

    fn ms(m: u64) -> usize {
        (f64::from(SAMPLE_RATE) * Duration::from_millis(m).as_secs_f64()) as usize
    }

    #[test]
    fn model_is_send_and_sync() {
        fn assert_ss<T: Send + Sync>() {}
        assert_ss::<Model>();
        assert_ss::<CancelToken>();
    }

    #[test]
    fn long_line_is_split_and_clause_pauses_inserted() {
        let (m, eng, _) = fake_model(true);
        let line = "abc, ".repeat(100);
        let opts = SynthOptions { max_phonemes: 40, ..Default::default() };
        let wav = m.synthesize(&line, &opts).unwrap();
        let calls = eng.calls.lock().unwrap();
        assert!(calls.len() > 5, "za mało porcji: {}", calls.len());
        assert!(calls.iter().all(|(ids, _, _)| ids.len() - 2 <= 40), "porcja dłuższa niż limit");
        let samples: usize = calls.iter().map(|(ids, _, _)| ids.len() * 100).sum();
        // jedna linia: między porcjami pauza „clause", po ostatniej nic
        assert_eq!(wav.len(), samples + (calls.len() - 1) * ms(80));
    }

    #[test]
    fn style_row_is_length_minus_one_and_unicode_counts_chars() {
        let (m, eng, _) = fake_model(true);
        m.synthesize("ɛɛɛ", &SynthOptions::default()).unwrap(); // 3 znaki (6 bajtów UTF-8)
        let calls = eng.calls.lock().unwrap();
        assert_eq!(calls[0].0, vec![0, 27, 27, 27, 0], "BOS + 3 id + EOS");
        assert_eq!(calls[0].1, 2, "wiersz stylu = liczba znaków - 1");
    }

    #[test]
    fn unknown_chars_are_dropped_and_empty_result_is_an_error() {
        let (m, eng, _) = fake_model(true);
        m.synthesize("a§b", &SynthOptions::default()).unwrap();
        assert_eq!(eng.calls.lock().unwrap()[0].0, vec![0, 1, 2, 0]);
        assert!(matches!(m.synthesize("§§§", &SynthOptions::default()), Err(Error::NoPhonemes)));
        assert!(matches!(m.synthesize("  \n\n ", &SynthOptions::default()), Err(Error::NoPhonemes)));
    }

    #[test]
    fn pauses_follow_line_and_paragraph_structure() {
        let (m, eng, _) = fake_model(true);
        // 3 zdania: A i B w jednym akapicie, C w drugim
        let wav = m.synthesize("aa\nbb\n\ncc", &SynthOptions::default()).unwrap();
        let n = eng.calls.lock().unwrap().len();
        assert_eq!(n, 3);
        // po A pauza zdaniowa, po B (koniec akapitu) akapitowa, po C (ostatnia) nic
        assert_eq!(wav.len(), 3 * 4 * 100 + ms(200) + ms(450));
    }

    #[test]
    fn options_are_validated_and_limits_clamped() {
        let (m, eng, _) = fake_model(true);
        for speed in [0.0, -2.0, f32::NAN, f32::NEG_INFINITY] {
            assert!(
                matches!(m.synthesize("a", &SynthOptions { speed, ..Default::default() }), Err(Error::InvalidOption(_))),
                "speed={speed}"
            );
        }
        // max_phonemes 0 -> 1 (nie panika); ogromny -> 510
        m.synthesize("abc", &SynthOptions { max_phonemes: 0, ..Default::default() }).unwrap();
        assert_eq!(eng.calls.lock().unwrap().len(), 3, "limit 0 traktowany jak 1: trzy porcje po znaku");
        eng.calls.lock().unwrap().clear();
        m.synthesize(&"a".repeat(600), &SynthOptions { max_phonemes: 99_999, ..Default::default() }).unwrap();
        let lens: Vec<usize> = eng.calls.lock().unwrap().iter().map(|c| c.0.len() - 2).collect();
        assert!(lens.iter().all(|&l| l <= STYLE_ROWS) && lens.len() >= 2, "{lens:?}");
    }

    #[test]
    fn speed_is_passed_to_engine() {
        let (m, eng, _) = fake_model(true);
        m.synthesize("a", &SynthOptions { speed: 1.75, ..Default::default() }).unwrap();
        assert_eq!(eng.calls.lock().unwrap()[0].2, 1.75);
    }

    #[test]
    fn unload_is_idempotent_closes_backend_and_blocks_use() {
        let (m, _, closed) = fake_model(true);
        assert!(m.is_loaded());
        m.unload().unwrap();
        m.unload().unwrap();
        assert!(!m.is_loaded() && closed.load(Ordering::SeqCst));
        assert!(matches!(m.synthesize("a", &SynthOptions::default()), Err(Error::Unloaded)));
        assert!(matches!(m.text_to_ipa("a", &TextOptions::default()), Err(Error::Unloaded)));
    }

    #[test]
    fn drop_closes_backend() {
        let (m, _, closed) = fake_model(true);
        drop(m);
        assert!(closed.load(Ordering::SeqCst), "Drop nie zwolnił modelu");
    }

    #[test]
    fn model_without_kokoro_only_phonemizes() {
        let (m, _, _) = fake_model(false);
        assert_eq!(m.text_to_ipa("Ala ma kota. Kot ma Ale.", &TextOptions::default()).unwrap(), "ala ma kota.\nkot ma ale.");
        assert!(matches!(m.synthesize("a", &SynthOptions::default()), Err(Error::NoKokoro)));
    }

    #[test]
    fn cancel_stops_synthesis_between_chunks() {
        let (m, eng, _) = fake_model(true);
        m.cancel_token().cancel();
        assert!(matches!(m.synthesize("aa\nbb", &SynthOptions::default()), Err(Error::Cancelled)));
        assert!(eng.calls.lock().unwrap().is_empty(), "silnik uruchomiony mimo anulowania");
    }

    #[test]
    fn default_synth_options_are_the_documented_values() {
        let o = SynthOptions::default();
        assert_eq!((o.speed, o.max_phonemes), (1.0, 300));
        assert_eq!(
            (o.sentence_pause, o.paragraph_pause, o.clause_pause),
            (Duration::from_millis(200), Duration::from_millis(450), Duration::from_millis(80))
        );
    }

    fn model_with(sel: Selection, with_g2p: bool) -> Model {
        let g2p: Option<Box<dyn G2p>> = with_g2p.then(|| Box::new(FakeG2p(Arc::new(AtomicBool::new(false)))) as Box<dyn G2p>);
        Model::from_parts(g2p, None, HashMap::new(), Vec::new(), CancelToken::new(), PathBuf::from("/x"), sel)
    }

    #[test]
    fn polish_normalization_and_splitting_only_for_polish_g2p() {
        let de = model_with(Selection { g2p_lang: Some("de".into()), ..Default::default() }, true);
        // bez normalizacji PL („5 zł” zostaje), wielka litera spoza ASCII/PL otwiera zdanie
        assert_eq!(de.text_to_ipa("Es kostet 5 zł. Über alles.", &TextOptions::default()).unwrap(), "es kostet 5 zł.\nüber alles.");
        let pl = model_with(Selection { g2p_lang: Some("pl".into()), ..Default::default() }, true);
        assert_eq!(pl.text_to_ipa("Mam 5 zł.", &TextOptions::default()).unwrap(), "mam 5 złotych.");
        assert_eq!(de.phonemis_lang(), Some("de"));
    }

    #[test]
    fn model_without_g2p_synthesizes_only_from_ipa() {
        let m = model_with(Selection { lang: Some("ja".into()), voice: Some("jf_alpha".into()), g2p_lang: None }, false);
        let err = m.text_to_ipa("こんにちは", &TextOptions::default()).unwrap_err();
        assert!(matches!(err, Error::Config(_)) && err.to_string().contains("--ipa"), "{err}");
        assert_eq!((m.lang(), m.voice(), m.phonemis_lang()), (Some("ja"), Some("jf_alpha"), None));
        m.unload().unwrap(); // brak backendu nie przeszkadza w zwalnianiu
    }
}
