//! Lokalna kopia modelu Kokoro w `<root>/<REVISION>/…` (ten sam układ co wersje Pythona i Go, więc pobrane pliki
//! można współdzielić przez `KOKORO_MODEL_DIR`). Gdy plik jest na dysku, sieć nie jest używana.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use log::info;
use serde::Deserialize;

use crate::cancel::CancelToken;
use crate::error::{Error, Result};
use crate::g2p::expand_home;
use crate::Config;

pub const REPO_ID: &str = "Shusek00/kokoro-kmp-models";
pub const REVISION: &str = "v2.1.1";
/// Domyślny język mówcy (i fonemizera), gdy `Config::lang` jest puste.
pub const DEFAULT_LANG: &str = "pl";
#[deprecated(note = "użyj DEFAULT_LANG albo Config::lang")]
pub const LANG_ID: &str = DEFAULT_LANG;
/// Częstotliwość próbkowania Kokoro-82M.
pub const SAMPLE_RATE: u32 = 24000;
/// Maksymalna liczba fonemów w jednym przebiegu (liczba wierszy macierzy stylu).
pub const STYLE_ROWS: usize = 510;
pub(crate) const STYLE_DIM: usize = 256;

pub(crate) struct Store {
    root: PathBuf,
    offline: bool,
    endpoint: String,
}

#[cfg(target_os = "macos")]
fn user_cache_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Caches"))
}

#[cfg(windows)]
fn user_cache_dir() -> Option<PathBuf> {
    std::env::var_os("LocalAppData").map(PathBuf::from)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn user_cache_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
}

fn default_model_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("KOKORO_MODEL_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(d);
    }
    match user_cache_dir() {
        Some(c) => c.join("kokoro-pl").join("kokoro-kmp-models"),
        None => PathBuf::from("models").join("kokoro-kmp-models"),
    }
}

/// Segment ścieżki URL: znaki niezarezerwowane bez zmian, reszta jako %XX.
fn encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Odpowiednik `filepath.IsLocal`: bez `..`, ścieżek bezwzględnych i pustych.
fn is_local(rel: &str) -> bool {
    !rel.is_empty() && Path::new(rel).components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

impl Store {
    pub(crate) fn new(cfg: &Config) -> Store {
        let root = cfg.model_dir.clone().unwrap_or_else(default_model_dir);
        let endpoint = cfg
            .hf_endpoint
            .clone()
            .or_else(|| std::env::var("HF_ENDPOINT").ok().filter(|v| !v.is_empty()))
            .unwrap_or_else(|| "https://huggingface.co".to_string());
        let offline_env = matches!(std::env::var("KOKORO_OFFLINE").unwrap_or_default().to_lowercase().as_str(), "1" | "true" | "yes");
        Store { root: expand_home(&root), offline: cfg.offline || offline_env, endpoint: endpoint.trim_end_matches('/').to_string() }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn base(&self) -> PathBuf {
        self.root.join(REVISION)
    }

    /// Zwraca ścieżkę lokalnego pliku: 1. jest na dysku -> zero sieci; 2. pobranie z HF (chyba że offline).
    pub(crate) fn get(&self, rel: &str, cancel: &CancelToken) -> Result<PathBuf> {
        if !is_local(rel) {
            return Err(Error::Config(format!("niebezpieczna ścieżka artefaktu w catalog.json: {rel:?}")));
        }
        let dest = self.base().join(rel);
        if std::fs::metadata(&dest).is_ok_and(|m| m.is_file() && m.len() > 0) {
            return Ok(dest); // zapis jest atomowy (.part -> rename), więc plik jest kompletny
        }
        if self.offline {
            return Err(Error::Config(format!(
                "brak lokalnego pliku modelu: {}\nPobierz raz z siecią: download_model (CLI: --fetch-only), katalog: {}",
                dest.display(),
                self.base().display()
            )));
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| self.write_error(rel, e))?;
        }
        let segs: Vec<String> = rel.split('/').map(encode_segment).collect();
        let url = format!("{}/{}/resolve/{}/{}", self.endpoint, REPO_ID, encode_segment(REVISION), segs.join("/"));
        info!("[model] pobieranie plik={rel} do={}", dest.parent().unwrap_or(Path::new(".")).display());

        // ureq nie ma domyślnych limitów czasu: bez nich niedostępny serwer blokowałby pobieranie w nieskończoność.
        // Ciało nie ma limitu całkowitego (pliki mają setki MB); jego odczyt jest w porcjach i sprawdza anulowanie.
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            .build()
            .into();
        cancel.check()?;
        let mut req = agent.get(&url).header("User-Agent", "plkokoro-rs");
        if let Some(tok) = std::env::var("HF_TOKEN").ok().filter(|t| !t.is_empty()) {
            req = req.header("Authorization", format!("Bearer {tok}"));
        }
        let mut resp = req.call().map_err(|e| match e {
            ureq::Error::StatusCode(code) => Error::Http(format!("pobieranie {rel}: HTTP {code} ({url})")),
            other => Error::Http(format!("pobieranie {rel}: {other}")),
        })?;
        let expected = resp.body().content_length();

        let part = PathBuf::from(format!("{}.part", dest.display()));
        let mut f = std::fs::File::create(&part).map_err(|e| self.write_error(rel, e))?;
        let copied = (|| -> Result<u64> {
            let mut reader = resp.body_mut().as_reader();
            let mut buf = vec![0u8; 64 * 1024];
            let mut total = 0u64;
            loop {
                cancel.check()?;
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                f.write_all(&buf[..n])?;
                total += n as u64;
            }
            f.flush()?;
            Ok(total)
        })();
        let n = match copied {
            Ok(n) => n,
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                return Err(match e {
                    Error::Cancelled => Error::Cancelled,
                    other => Error::Http(format!("pobieranie {rel}: {other}")),
                });
            }
        };
        let problem = match expected {
            Some(exp) if exp != n => Some(format!("pobrano {n} z {exp} bajtów")),
            _ if n == 0 => Some("serwer zwrócił pusty plik".to_string()),
            _ => None,
        };
        if let Some(p) = problem {
            let _ = std::fs::remove_file(&part);
            return Err(Error::Http(format!("pobieranie {rel}: {p}")));
        }
        std::fs::rename(&part, &dest)?;
        info!("[model] zapisano plik={} MB={:.1}", dest.display(), n as f64 / 1e6);
        Ok(dest)
    }

    /// Brak prawa zapisu (np. model z pakietu Nix w /nix/store) to częsty przypadek po wybraniu głosu spoza pakietu.
    fn write_error(&self, rel: &str, e: std::io::Error) -> Error {
        if e.kind() != std::io::ErrorKind::PermissionDenied && e.kind() != std::io::ErrorKind::ReadOnlyFilesystem {
            return e.into();
        }
        Error::Config(format!(
            "brak pliku modelu {rel}, a katalog {} jest tylko do odczytu ({e}).\nJeśli model pochodzi z pakietu Nix, dodaj \
             głos do plkokoro.voices w devenv.local.nix; albo ustaw KOKORO_MODEL_DIR / Config::model_dir na katalog z prawem zapisu",
            self.root.display()
        ))
    }
}

// --- catalog.json (tylko używane pola) -------------------------------------------------------------

#[derive(Deserialize)]
struct Artifact {
    path: String,
}
#[derive(Deserialize)]
struct Voice {
    id: String,
    #[serde(rename = "displayName", default)]
    display_name: String,
    #[serde(default)]
    gender: String,
    #[serde(rename = "modelId")]
    model_id: String,
    artifact: Artifact,
}
/// Frontend tekstowy języka: `status = "bundled"` + `language` (kod Phonemis) albo `"external-required"` (ja, zh).
#[derive(Deserialize, Default)]
struct TextFrontend {
    #[serde(default)]
    status: String,
    #[serde(default)]
    language: String,
}
#[derive(Deserialize)]
struct Language {
    id: String,
    #[serde(rename = "defaultVoiceId", default)]
    default_voice_id: String,
    #[serde(rename = "textFrontend", default)]
    text_frontend: Option<TextFrontend>,
    #[serde(default)]
    voices: Vec<Voice>,
}

impl Language {
    /// Kod języka dla Phonemis; `None`, gdy katalog wymaga zewnętrznego frontendu (ja, zh). Bez `textFrontend`
    /// przyjmujemy id języka (stare i testowe katalogi).
    fn g2p_lang(&self) -> Option<String> {
        match &self.text_frontend {
            None => Some(self.id.clone()),
            Some(t) if t.status == "external-required" => None,
            Some(t) if !t.language.is_empty() => Some(t.language.clone()),
            Some(_) => Some(self.id.clone()),
        }
    }
}

/// Głos z `catalog.json` (patrz `list_voices`).
#[derive(Clone, Debug)]
pub struct VoiceInfo {
    /// Język mówcy (id z katalogu, np. `pl`, `en-us`).
    pub lang: String,
    /// Id głosu (np. `pm_mateusz`) — wartość dla `Config::voice`.
    pub id: String,
    pub display_name: String,
    /// `female` / `male` (jak w katalogu; może być puste).
    pub gender: String,
    /// Model ONNX, którego wymaga głos.
    pub model_id: String,
    /// Czy to domyślny głos języka.
    pub is_default: bool,
    /// Język Phonemis dla tego języka mówcy; `None` = brak frontendu (podaj IPA albo własny `Config::g2p`).
    pub g2p_lang: Option<String>,
}
#[derive(Deserialize)]
struct ModelEntry {
    id: String,
    #[serde(rename = "tokenizerId")]
    tokenizer_id: String,
    artifact: Artifact,
}
#[derive(Deserialize)]
struct Tokenizer {
    id: String,
    artifact: Artifact,
}
#[derive(Deserialize)]
struct TokenEncoding {
    #[serde(rename = "vocabularyField")]
    vocabulary_field: String,
}
#[derive(Deserialize)]
struct Runtime {
    #[serde(rename = "tokenEncoding")]
    token_encoding: TokenEncoding,
}
#[derive(Deserialize)]
pub(crate) struct Catalog {
    runtime: Runtime,
    languages: Vec<Language>,
    models: Vec<ModelEntry>,
    tokenizers: Vec<Tokenizer>,
}

/// Wybrany język i głos oraz ścieżki względne ich artefaktów.
pub(crate) struct Resolved {
    pub lang: String,
    pub voice_id: String,
    pub g2p_lang: Option<String>,
    pub voice: String,
    pub model: String,
    pub tokenizer: String,
    pub vocab_field: String,
}

impl Catalog {
    /// Język mówcy `lang` i głos `voice` (domyślnie `defaultVoiceId` języka albo pierwszy głos).
    pub(crate) fn resolve(&self, lang: &str, voice: Option<&str>) -> Result<Resolved> {
        let bad = |what: String| Error::Config(format!("catalog.json ({REVISION}): {what}"));
        let lang = self.languages.iter().find(|l| l.id == lang).ok_or_else(|| {
            let ids: Vec<&str> = self.languages.iter().map(|l| l.id.as_str()).collect();
            bad(format!("nie znaleziono języka {lang:?}; dostępne: {}", ids.join(", ")))
        })?;
        let voice = match voice {
            Some(want) => lang.voices.iter().find(|v| v.id == want).ok_or_else(|| {
                let ids: Vec<&str> = lang.voices.iter().map(|v| v.id.as_str()).collect();
                let hint = match self.languages.iter().find(|l| l.voices.iter().any(|v| v.id == want)) {
                    Some(other) => format!(" (głos {want:?} należy do języka {:?} — ustaw Config::lang / --lang)", other.id),
                    None => String::new(),
                };
                bad(format!("język {:?} nie ma głosu {want:?}{hint}; dostępne: {}", lang.id, ids.join(", ")))
            })?,
            None => lang
                .voices
                .iter()
                .find(|v| v.id == lang.default_voice_id)
                .or(lang.voices.first())
                .ok_or_else(|| bad(format!("język {:?} nie ma głosów", lang.id)))?,
        };
        let model = self
            .models
            .iter()
            .find(|m| m.id == voice.model_id)
            .ok_or_else(|| bad(format!("nie znaleziono modelu {:?}", voice.model_id)))?;
        let tok = self
            .tokenizers
            .iter()
            .find(|t| t.id == model.tokenizer_id)
            .ok_or_else(|| bad(format!("nie znaleziono tokenizera {:?}", model.tokenizer_id)))?;
        Ok(Resolved {
            lang: lang.id.clone(),
            voice_id: voice.id.clone(),
            g2p_lang: lang.g2p_lang(),
            voice: voice.artifact.path.clone(),
            model: model.artifact.path.clone(),
            tokenizer: tok.artifact.path.clone(),
            vocab_field: self.runtime.token_encoding.vocabulary_field.clone(),
        })
    }

    /// Wszystkie głosy katalogu, w kolejności języków i głosów z pliku.
    pub(crate) fn voices(&self) -> Vec<VoiceInfo> {
        let mut out = Vec::new();
        for l in &self.languages {
            let g2p_lang = l.g2p_lang();
            for v in &l.voices {
                out.push(VoiceInfo {
                    lang: l.id.clone(),
                    id: v.id.clone(),
                    display_name: v.display_name.clone(),
                    gender: v.gender.clone(),
                    model_id: v.model_id.clone(),
                    is_default: v.id == l.default_voice_id,
                    g2p_lang: g2p_lang.clone(),
                });
            }
        }
        out
    }
}

impl Store {
    pub(crate) fn load_catalog(&self, cancel: &CancelToken) -> Result<Catalog> {
        let p = self.get("catalog.json", cancel)?;
        serde_json::from_slice(&std::fs::read(p)?).map_err(|e| Error::Json(format!("catalog.json: {e}")))
    }

    /// Plik tokenizera zawiera słownik `{znak: id}`; nazwę pola wskazuje `catalog.runtime.tokenEncoding`.
    pub(crate) fn load_vocab(&self, r: &Resolved, cancel: &CancelToken) -> Result<HashMap<char, i64>> {
        let p = self.get(&r.tokenizer, cancel)?;
        let cfg: serde_json::Value =
            serde_json::from_slice(&std::fs::read(p)?).map_err(|e| Error::Json(format!("{}: {e}", r.tokenizer)))?;
        let field = cfg
            .get(&r.vocab_field)
            .and_then(|v| v.as_object())
            .ok_or_else(|| Error::Json(format!("{}: brak pola {:?} (obiekt)", r.tokenizer, r.vocab_field)))?;
        let mut vocab = HashMap::with_capacity(field.len());
        for (k, v) in field {
            let mut it = k.chars();
            // jak w Pythonie: kodujemy znak po znaku, więc klucze wieloznakowe są pomijane
            if let (Some(c), None) = (it.next(), it.next()) {
                let id = v.as_i64().ok_or_else(|| Error::Json(format!("{}: id znaku {k:?} nie jest liczbą całkowitą", r.tokenizer)))?;
                vocab.insert(c, id);
            }
        }
        Ok(vocab)
    }

    /// Głos: macierz `STYLE_ROWS × STYLE_DIM` float32 little-endian.
    pub(crate) fn load_style(&self, r: &Resolved, cancel: &CancelToken) -> Result<Vec<f32>> {
        let p = self.get(&r.voice, cancel)?;
        let raw = std::fs::read(p)?;
        let want = STYLE_ROWS * STYLE_DIM * 4;
        if raw.len() != want {
            return Err(Error::Config(format!(
                "{}: rozmiar {} B, oczekiwano {want} B ({STYLE_ROWS}×{STYLE_DIM} float32)",
                r.voice,
                raw.len()
            )));
        }
        Ok(raw.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
    }
}
