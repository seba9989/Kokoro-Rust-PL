//! plkokoro — wiersz poleceń dla biblioteki `plkokoro` (load_model / text_to_ipa / synthesize / unload).
//!
//! ```text
//! plkokoro --runner /…/Phonemis/build/phonemis_runner "Cześć, to jest test."
//! plkokoro --phonemize-only "Mam 123 zł, 5% rabatu."      # tylko IPA, bez Kokoro
//! plkokoro --ipa "kɔkˈɔrɔ" -o kokoro.wav                   # synteza prosto z IPA
//! plkokoro --fetch-only                                     # pobierz model Kokoro do katalogu lokalnego
//! ```
//! Kody wyjścia: 0 sukces, 1 błąd działania, 2 błędne użycie (flagi).

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use clap::Parser;
use plkokoro::{download_model, load_model, write_wav, CancelToken, Config, SynthOptions, TextOptions, SAMPLE_RATE, STYLE_ROWS};

const DEFAULT_TEXT: &str = "Cześć, to jest test polskiej syntezy mowy Kokoro.";

#[derive(Parser, Debug)]
#[command(name = "plkokoro", about = "Polski TTS: Kokoro (ONNX) + Phonemis", disable_version_flag = true)]
struct Args {
    /// Tekst do syntezy (kilka słów jest łączonych spacją); domyślnie zdanie testowe.
    text: Vec<String>,

    /// Wczytaj tekst z pliku (UTF-8) zamiast z argumentów.
    #[arg(short = 'f', long = "file")]
    file: Option<PathBuf>,

    /// Plik wyjściowy WAV.
    #[arg(short = 'o', long = "output", default_value = "output_pl.wav")]
    output: PathBuf,

    /// Tempo mowy (> 0).
    #[arg(long, default_value_t = 1.0, allow_negative_numbers = true)]
    speed: f64,

    /// Limit fonemów na porcję (maks. 510).
    #[arg(long = "max-phonemes", default_value_t = 300)]
    max_phonemes: usize,

    /// Wyłącz normalizację uzupełniającą (zł, %, skróty…).
    #[arg(long = "no-normalize")]
    no_normalize: bool,

    /// Wejście to gotowe IPA — pomiń text_to_ipa.
    #[arg(long)]
    ipa: bool,

    /// Wypisz IPA i zakończ (bez ładowania Kokoro).
    #[arg(long = "phonemize-only")]
    phonemize_only: bool,

    /// Pobierz model Kokoro do --model-dir i zakończ.
    #[arg(long = "fetch-only")]
    fetch_only: bool,

    /// Katalog lokalnej kopii modelu (albo KOKORO_MODEL_DIR).
    #[arg(long = "model-dir")]
    model_dir: Option<PathBuf>,

    /// Nigdy nie łącz się z HF (albo KOKORO_OFFLINE=1).
    #[arg(long)]
    offline: bool,

    /// Providery ONNX Runtime po przecinku, np. CUDAExecutionProvider (wymaga odpowiedniej cechy kompilacji).
    #[arg(long, value_delimiter = ',')]
    providers: Vec<String>,

    /// Ścieżka do phonemis_runner (albo PHONEMIS_RUNNER).
    #[arg(long)]
    runner: Option<PathBuf>,

    /// Ścieżka do phonemizer_pl.bin (albo PHONEMIS_MODEL).
    #[arg(long = "model")]
    weights: Option<PathBuf>,

    /// Ścieżka do libonnxruntime.so (albo ORT_LIBRARY_PATH).
    #[arg(long = "ort-lib")]
    ort_lib: Option<PathBuf>,

    /// Pokaż normalizację i IPA każdego zdania.
    #[arg(short = 'v', long)]
    verbose: bool,
}

struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::max_level()
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            eprintln!("level={} msg={}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}

static LOGGER: StderrLogger = StderrLogger;

fn setup_logger(verbose: bool) {
    let level = if verbose { log::LevelFilter::Debug } else { log::LevelFilter::Info };
    let _ = log::set_logger(&LOGGER).map(|()| log::set_max_level(level));
}

/// Ctrl+C / SIGTERM: pierwsze anuluje kooperacyjnie (zabija podprocesy fonemizacji, zatrzymuje syntezę),
/// drugie kończy proces natychmiast (np. gdy zawiesza się pobieranie).
fn install_interrupt(cancel: &CancelToken) {
    let cancel = cancel.clone();
    let already = AtomicBool::new(false);
    let _ = ctrlc::set_handler(move || {
        if already.swap(true, Ordering::SeqCst) {
            std::process::exit(130);
        }
        cancel.cancel();
    });
}

fn fail(err: impl std::fmt::Display) -> u8 {
    eprintln!("błąd: {err}");
    1
}

fn run(a: Args) -> u8 {
    setup_logger(a.verbose || a.phonemize_only);
    let cancel = CancelToken::new();
    install_interrupt(&cancel);

    let cfg = Config {
        model_dir: a.model_dir,
        offline: a.offline,
        phonemis_runner: a.runner,
        phonemis_weights: a.weights,
        ort_library: a.ort_lib,
        providers: a.providers,
        skip_kokoro: a.phonemize_only,
        cancel: Some(cancel),
        ..Default::default()
    };

    if a.fetch_only {
        return match download_model(&cfg) {
            Ok(paths) => {
                for p in paths {
                    if let Ok(m) = std::fs::metadata(&p) {
                        println!("{}  ({:.1} MB)", p.display(), m.len() as f64 / 1e6);
                    }
                }
                0
            }
            Err(e) => fail(e),
        };
    }

    let text = if let Some(f) = &a.file {
        match std::fs::read_to_string(f) {
            Ok(t) => t,
            Err(e) => return fail(format!("{}: {e}", f.display())),
        }
    } else if !a.text.is_empty() {
        a.text.join(" ")
    } else {
        DEFAULT_TEXT.to_string()
    };

    let model = match load_model(cfg) {
        Ok(m) => m,
        Err(e) => return fail(e),
    };
    let ipa = if a.ipa {
        text
    } else {
        match model.text_to_ipa(&text, &TextOptions { no_normalize: a.no_normalize }) {
            Ok(i) => i,
            Err(e) => return fail(e),
        }
    };
    if a.phonemize_only {
        println!("{ipa}");
        return 0;
    }
    let opts = SynthOptions { speed: a.speed as f32, max_phonemes: a.max_phonemes.min(STYLE_ROWS), ..Default::default() };
    let t0 = Instant::now();
    let wav = match model.synthesize(&ipa, &opts) {
        Ok(w) => w,
        Err(e) => return fail(e),
    };
    let synth_time = t0.elapsed();
    if let Err(e) = write_wav(&a.output, &wav) {
        return fail(format!("{}: {e}", a.output.display()));
    }
    let _ = model.unload();
    println!(
        "Zapisano {} ({:.1} s nagrania, wygenerowano w {:.1} s)",
        a.output.display(),
        wav.len() as f64 / f64::from(SAMPLE_RATE),
        synth_time.as_secs_f64()
    );
    0
}

fn main() -> ExitCode {
    // clap kończy z kodem 2 przy błędnych flagach i 0 przy --help
    ExitCode::from(run(Args::parse()))
}
