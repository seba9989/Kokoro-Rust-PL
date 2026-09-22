# Referencja biblioteki (`plkokoro`)

```toml
[dependencies]
plkokoro = { path = "…/plkokoro-rs/plkokoro" }
# providery poza CPU: features = ["cuda"]  (patrz „Cechy Cargo”)
```

Wszystkie operacje są synchroniczne i blokujące. `Model` jest `Send + Sync` (można go współdzielić między wątkami,
np. w `Arc<Model>` albo przez `thread::scope`).

## Cztery operacje

```rust
use plkokoro::{load_model, Config, SynthOptions, TextOptions};

let model = load_model(Config { phonemis_runner: Some("…/phonemis_runner".into()), ..Default::default() })?; // 1
let ipa   = model.text_to_ipa("Cześć, [Kokoro](/kɔkˈɔrɔ/).", &TextOptions::default())?;                        // 2
let wav   = model.synthesize(&ipa, &SynthOptions::default())?;                                                 // 3
model.unload()?;                                                                                               // 4
plkokoro::write_wav("out.wav", &wav)?;
```

| Operacja | Sygnatura | Uwagi |
|---|---|---|
| 1. ładowanie | `load_model(cfg: Config) -> Result<Model>` | najpierw backend G2P (błąd konfiguracji przed pobieraniem), potem model; brakujące pliki są pobierane raz |
| 2. tekst → IPA | `Model::text_to_ipa(&self, text: &str, opts: &TextOptions) -> Result<String>` | jedno zdanie na linię, pusta linia = akapit; puste wejście → `""`; nie filtruje IPA słownikiem |
| 3. IPA → audio | `Model::synthesize(&self, ipa: &str, opts: &SynthOptions) -> Result<Vec<f32>>` | mono `f32`, `SAMPLE_RATE` Hz; znaki spoza słownika pomijane z ostrzeżeniem (raz na znak) |
| 4. zwolnienie | `Model::unload(&self) -> Result<()>` | idempotentne; czeka na trwające wywołania; `Drop` robi to samo. Uwaga: biblioteka ORT zostaje w pamięci ([dzialanie.md](dzialanie.md) §6) |

Dodatkowo: `download_model(&Config) -> Result<Vec<PathBuf>>` (tylko pobranie plików modelu, bez ładowania),
`Model::is_loaded()`, `Model::model_dir()`, `Model::cancel_token()`.

## `Config`

Wszystkie pola opcjonalne (`Config::default()` + zmienne środowiskowe wystarczają). Struktura nie jest `Clone`
(zawiera `Box<dyn G2p>`); buduj ją z `..Default::default()`.

| Pole | Typ | Domyślnie | Znaczenie |
|---|---|---|---|
| `model_dir` | `Option<PathBuf>` | `KOKORO_MODEL_DIR` albo `<cache>/kokoro-pl/kokoro-kmp-models` | lokalna kopia modelu (`<dir>/v2.1.1/…`) |
| `offline` | `bool` | `false` (`KOKORO_OFFLINE=1` też włącza) | nigdy nie łącz się z HF |
| `hf_endpoint` | `Option<String>` | `HF_ENDPOINT` albo `https://huggingface.co` | serwer z modelem (token: `HF_TOKEN`) |
| `phonemis_runner` | `Option<PathBuf>` | `PHONEMIS_RUNNER` | ścieżka do `phonemis_runner` |
| `phonemis_weights` | `Option<PathBuf>` | `PHONEMIS_MODEL`, potem wyprowadzone z runnera | `phonemizer_pl.bin` |
| `g2p` | `Option<Box<dyn G2p>>` | — | własny backend zamiast runnera (zamykany w `unload`) |
| `workers` | `Option<usize>` | `min(8, CPU)` | równoległość fonemizacji |
| `ort_library` | `Option<PathBuf>` | `ORT_LIBRARY_PATH`, potem `ORT_DYLIB_PATH`, potem `libonnxruntime.so` z ścieżek systemowych | biblioteka ONNX Runtime (>= 1.21) |
| `providers` | `Vec<String>` | CPU | np. `["CUDAExecutionProvider"]`; wymaga cechy Cargo |
| `skip_kokoro` | `bool` | `false` | tylko G2P: bez pobierania i ładowania Kokoro; `synthesize` → `Error::NoKokoro` |
| `cancel` | `Option<CancelToken>` | nowy | token współdzielony z wywołującym |

## `TextOptions`, `SynthOptions`

```rust
TextOptions { no_normalize: false }                       // Default
SynthOptions {
    speed: 1.0,                                           // > 0, inaczej Error::InvalidOption
    max_phonemes: 300,                                    // przycinane do 1..=510
    sentence_pause: Duration::from_millis(200),
    paragraph_pause: Duration::from_millis(450),
    clause_pause: Duration::from_millis(80),
}                                                         // Default
```

Zmieniaj wybrane pola składnią `SynthOptions { speed: 1.2, ..Default::default() }` — pauzy zostają domyślne.
`Duration::ZERO` oznacza brak pauzy. (W wersji Go `&SynthOptions{Speed: 1}` zerowało pauzy; tu ta pułapka nie istnieje.)

## Błędy (`Error`)

`plkokoro::Result<T> = Result<T, plkokoro::Error>`. Warianty do dopasowania w kodzie:

| Wariant | Kiedy |
|---|---|
| `Unloaded` | operacja po `unload` |
| `NoKokoro` | `synthesize` na modelu z `skip_kokoro` |
| `NoPhonemes` | brak czego syntezować (puste wejście albo same znaki spoza słownika) |
| `Cancelled` | `CancelToken::cancel()` |
| `InvalidOption(String)` | np. `speed <= 0` albo `NaN` |
| `Config(String)` | brak runnera/wag/ORT, tryb offline bez pliku, niebezpieczna ścieżka w `catalog.json`, nieznany provider |
| `Http`, `Json`, `Io`, `Runner`, `Ort` | błąd pobierania / parsowania / systemu plików / fonemizacji / ONNX Runtime; komunikaty zawierają podpowiedź naprawy |

`Error` implementuje `Display` i `std::error::Error` (`Io` udostępnia `source()`).

## `CancelToken`

```rust
let token = CancelToken::new();
let model = load_model(Config { cancel: Some(token.clone()), ..Default::default() })?;
std::thread::spawn(move || { /* … */ token.cancel(); });   // z dowolnego wątku / obsługi Ctrl+C
```

Po `cancel()`: fonemizacja zabija podprocesy, synteza zatrzymuje się między porcjami, pobieranie między porcjami danych;
wszystkie zwracają `Error::Cancelled`. **Token nie jest resetowany** — po anulowaniu ten sam `Model` odmawia dalszej
pracy; utwórz nowy token i model. Pojedynczej inferencji i zablokowanego odczytu z sieci nie da się przerwać.

## Własny backend G2P

```rust
struct Lower;
impl plkokoro::G2p for Lower {
    fn phonemize(&self, text: &str, _cancel: &CancelToken) -> plkokoro::Result<String> { Ok(text.to_lowercase()) }
    // opcjonalnie: phonemize_many (domyślnie sekwencyjnie), close()
}
let model = load_model(Config { g2p: Some(Box::new(Lower)), skip_kokoro: true, ..Default::default() })?;
```

Backend dostaje zwykły tekst (jedno zdanie lub jego kawałek): wstawki `[tekst](/ipa/)` obsługuje biblioteka. Wyniki są
cache'owane po tekście (do 20 000 wpisów). Pełny przykład: `plkokoro/examples/custom_g2p.rs`.
Wbudowany backend: `RunnerG2p::new(runner, wagi: Option<PathBuf>, workers: Option<usize>)` (limit 60 s na wywołanie).

## Pozostałe

| Element | Opis |
|---|---|
| `normalize_pl(&str) -> String` | normalizacja uzupełniająca (zł, %, skróty…) — używana przez `text_to_ipa`, dostępna osobno |
| `encode_wav(&mut impl Write, &[f32], sample_rate) -> io::Result<()>` | WAV PCM 16-bit mono; próbki poza `[-1, 1]` obcinane |
| `write_wav(path, &[f32]) -> io::Result<()>` | jw., do pliku, `SAMPLE_RATE` |
| stałe | `SAMPLE_RATE` (24000), `STYLE_ROWS` (510 = twardy limit fonemów na porcję), `REPO_ID`, `REVISION` (`v2.1.1`), `LANG_ID` (`pl`) |
| logi | fasada `log` (`[model]`, `[normalize]`, `[phonemis]`, `[uwaga]`); bez skonfigurowanego loggera nic nie jest wypisywane |

## Cechy Cargo

| Cecha | Efekt |
|---|---|
| `cuda`, `tensorrt`, `rocm`, `migraphx`, `coreml`, `directml`, `openvino` | włącza odpowiedni provider (nazwa w `Config::providers`, wielkość liter i sufiks `ExecutionProvider` bez znaczenia). Faktyczna dostępność zależy od załadowanej `libonnxruntime.so`; niedostępny provider daje `Error::Ort` (nie ciche CPU) |

Wszystkie siedem cech przechodzi `cargo check`; brak providera w bibliotece ORT daje błąd — sprawdzone dla `migraphx`
na ORT 1.22.0 i 1.30.0 (CPU). **Nie uruchamiano na prawdziwym GPU.** CLI przekazuje te cechy (`cargo build -p plkokoro-cli --features migraphx`).
