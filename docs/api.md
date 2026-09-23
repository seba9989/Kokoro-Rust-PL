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
| 1. ładowanie | `load_model(cfg: Config) -> Result<Model>` | najpierw `catalog.json` (wybór języka i głosu) i backend G2P (błąd konfiguracji przed pobieraniem kilkuset MB), potem model; brakujące pliki są pobierane raz |
| 2. tekst → IPA | `Model::text_to_ipa(&self, text: &str, opts: &TextOptions) -> Result<String>` | jedno zdanie na linię, pusta linia = akapit; puste wejście → `""`; nie filtruje IPA słownikiem |
| 3. IPA → audio | `Model::synthesize(&self, ipa: &str, opts: &SynthOptions) -> Result<Vec<f32>>` | mono `f32`, `SAMPLE_RATE` Hz; znaki spoza słownika pomijane z ostrzeżeniem (raz na znak) |
| 4. zwolnienie | `Model::unload(&self) -> Result<()>` | idempotentne; czeka na trwające wywołania; `Drop` robi to samo. Uwaga: biblioteka ORT zostaje w pamięci ([dzialanie.md](dzialanie.md) §6) |

Dodatkowo: `download_model(&Config) -> Result<Vec<PathBuf>>` (tylko pobranie plików wybranego głosu i modelu, bez
ładowania), `list_voices(&Config) -> Result<Vec<VoiceInfo>>` (głosy z `catalog.json`), `Model::is_loaded()`,
`Model::model_dir()`, `Model::cancel_token()`, `Model::lang()`, `Model::voice()`, `Model::phonemis_lang()`.

## Język mówcy, głos, język fonemizera

Trzy niezależne wybory (domyślnie wszystko polskie):

```rust
// niemiecki głos, niemiecki fonemizer (wynika z języka mówcy)
load_model(Config { lang: Some("de".into()), voice: Some("df_anna".into()), ..Default::default() })?;
// polski tekst czytany głosem amerykańskim (akcent zamierzony)
load_model(Config { lang: Some("en-us".into()), voice: Some("af_heart".into()), phonemis_lang: Some("pl".into()), ..Default::default() })?;
```

- **Język mówcy** (`lang`) to id języka z `catalog.json` (`pl`, `de`, `en-us`, `en-gb`, `es`, `fr`, `hi`, `it`, `ja`,
  `pt-br`, `zh`). Wyznacza głos, **model ONNX** i tokenizer: `pl` i `de` mają własne modele, reszta wspólny
  `kokoro-v1.0`, większość głosów `zh` (i np. `af_maple`) model `kokoro-v1.1-zh` z innym słownikiem.
- **Głos** (`voice`) musi należeć do języka mówcy; bez niego — `defaultVoiceId` języka. Błąd wymienia dostępne głosy
  (i podpowiada język, jeśli głos istnieje gdzie indziej).
- **Język fonemizera** (`phonemis_lang`) to kod profilu Phonemis (`PHONEMIS_LANGS`: `pl`, `en-us`, `en-gb`, `de`, `fr`,
  `es`, `it`, `pt`, `hi`). Domyślnie `textFrontend.language` języka mówcy z katalogu (`pt-br` → `pt`); przy
  `skip_kokoro` bez `lang` — `pl`. Dla `ja`/`zh` katalog nie ma frontendu: model ładuje się **bez G2P** (runner nie
  jest wymagany), `synthesize` działa na gotowym IPA, a `text_to_ipa` zwraca `Error::Config` z instrukcją.
- Normalizacja `normalize_pl` i polska reguła podziału na zdania działają tylko dla fonemizera `pl`; dla innych
  języków tekst idzie do Phonemis bez normalizacji, a zdanie zaczyna każda wielka litera Unicode.

`VoiceInfo`: `lang`, `id`, `display_name`, `gender`, `model_id`, `is_default`, `g2p_lang: Option<String>`.

## `Config`

Wszystkie pola opcjonalne (`Config::default()` + zmienne środowiskowe wystarczają). Struktura nie jest `Clone`
(zawiera `Box<dyn G2p>`); buduj ją z `..Default::default()`.

| Pole | Typ | Domyślnie | Znaczenie |
|---|---|---|---|
| `model_dir` | `Option<PathBuf>` | `KOKORO_MODEL_DIR` albo `<cache>/kokoro-pl/kokoro-kmp-models` | lokalna kopia modelu (`<dir>/v2.1.1/…`) |
| `offline` | `bool` | `false` (`KOKORO_OFFLINE=1` też włącza) | nigdy nie łącz się z HF |
| `hf_endpoint` | `Option<String>` | `HF_ENDPOINT` albo `https://huggingface.co` | serwer z modelem (token: `HF_TOKEN`) |
| `lang` | `Option<String>` | `KOKORO_LANG`, potem `DEFAULT_LANG` (`pl`) | język mówcy (id z `catalog.json`) |
| `voice` | `Option<String>` | `KOKORO_VOICE`, potem głos domyślny języka | głos (`list_voices`) |
| `phonemis_lang` | `Option<String>` | `PHONEMIS_LANG`, potem frontend języka mówcy z katalogu | język fonemizera Phonemis |
| `phonemis_runner` | `Option<PathBuf>` | `PHONEMIS_RUNNER` | ścieżka do `phonemis_runner` |
| `phonemis_weights` | `Option<PathBuf>` | `PHONEMIS_MODEL`, potem `<repo>/data/<język>/phonemizer_<język>.bin` obok runnera | wagi języka fonemizera |
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
| `Config(String)` | brak runnera/wag/ORT, nieznany język/głos, język bez Phonemis w `text_to_ipa`, tryb offline bez pliku, katalog modelu tylko do odczytu (np. `/nix/store`) bez wybranego głosu, niebezpieczna ścieżka w `catalog.json`, nieznany provider |
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
Wbudowany backend: `RunnerG2p::new(runner, lang: &str, wagi: Option<PathBuf>, workers: Option<usize>)` (limit 60 s na
wywołanie; `lang` z `PHONEMIS_LANGS`). Dla `en-us`/`en-gb` `lexicon_full.json` i `tagger.json` leżące obok wag są
przekazywane runnerowi (`--lexicon`, `--tagger`). `phonemis_weights_file(lang)` zwraca nazwę pliku wag.

## Pozostałe

| Element | Opis |
|---|---|
| `normalize_pl(&str) -> String` | normalizacja uzupełniająca (zł, %, skróty…) — używana przez `text_to_ipa` dla fonemizera `pl`, dostępna osobno |
| `encode_wav(&mut impl Write, &[f32], sample_rate) -> io::Result<()>` | WAV PCM 16-bit mono; próbki poza `[-1, 1]` obcinane |
| `write_wav(path, &[f32]) -> io::Result<()>` | jw., do pliku, `SAMPLE_RATE` |
| stałe | `SAMPLE_RATE` (24000), `STYLE_ROWS` (510 = twardy limit fonemów na porcję), `REPO_ID`, `REVISION` (`v2.1.1`), `DEFAULT_LANG` (`pl`; stare `LANG_ID` — przestarzałe), `PHONEMIS_LANGS` |
| logi | fasada `log` (`[model]`, `[normalize]`, `[phonemis]`, `[uwaga]`); bez skonfigurowanego loggera nic nie jest wypisywane |

## Cechy Cargo

| Cecha | Efekt |
|---|---|
| `cuda`, `tensorrt`, `rocm`, `migraphx`, `coreml`, `directml`, `openvino` | włącza odpowiedni provider (nazwa w `Config::providers`, wielkość liter i sufiks `ExecutionProvider` bez znaczenia). Faktyczna dostępność zależy od załadowanej `libonnxruntime.so`; niedostępny provider daje `Error::Ort` (nie ciche CPU) |

Wszystkie siedem cech przechodzi `cargo check`; brak providera w bibliotece ORT daje błąd — sprawdzone dla `migraphx`
na ORT 1.22.0 i 1.30.0 (CPU). **Nie uruchamiano na prawdziwym GPU.** CLI przekazuje te cechy (`cargo build -p plkokoro-cli --features migraphx`).
