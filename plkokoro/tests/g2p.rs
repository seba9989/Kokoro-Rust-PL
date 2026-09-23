//! Backend `phonemis_runner`: parsowanie wyjścia, wyprowadzanie wag, błędy konfiguracji, równoległość, anulowanie.
//! Używa fałszywego runnera (skrypt bash); zachowanie sterowane treścią tekstu: BOOM / SLOW / HANG.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use common::*;
use plkokoro::{CancelToken, Error, G2p, RunnerG2p};

#[test]
fn runner_basics() {
    let tmp = tempfile::tempdir().unwrap();
    let (runner, weights) = fake_runner(tmp.path());
    let g = RunnerG2p::new(&runner, "pl", None, None).unwrap(); // wagi wyprowadzone z układu repo
    assert_eq!(g.weights().canonicalize().unwrap(), weights.canonicalize().unwrap());
    let c = CancelToken::new();
    assert_eq!(g.phonemize("Cześć, ŚWIAT! abc", &c).unwrap(), "cześć, Świat! abc"); // tr zmienia tylko ASCII A-Z
    assert_eq!(g.phonemize("  \0 ", &c).unwrap(), "");
    assert_eq!(g.phonemize("-5 stopni", &c).unwrap(), "-5 stopni", "tekst z '-' na początku nie może być wzięty za opcję");
    match g.phonemize("BOOM", &c) {
        Err(Error::Runner(m)) => assert!(m.contains("kodem 1") && m.contains("zepsuty model"), "{m}"),
        other => panic!("kod != 0 powinien dać Error::Runner z komunikatem runnera: {other:?}"),
    }
}

#[test]
fn runner_config_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let (runner, weights) = fake_runner(tmp.path());

    let err = |r: plkokoro::Result<RunnerG2p>| r.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err(RunnerG2p::new("/nie/ma/runnera", "pl", None, None)).contains("brak phonemis_runner"));

    let noexec = tmp.path().join("r");
    std::fs::write(&noexec, "x").unwrap();
    std::fs::set_permissions(&noexec, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(err(RunnerG2p::new(&noexec, "pl", Some(weights.clone()), None)).contains("wykonywalnym"));

    // wskaźnik Git LFS zamiast wag
    let lfs = tmp.path().join("w.bin");
    std::fs::write(&lfs, "version https://git-lfs.github.com/spec/v1\noid sha256:x\nsize 7094120\n").unwrap();
    assert!(err(RunnerG2p::new(&runner, "pl", Some(lfs), None)).contains("Git LFS"));
    assert!(err(RunnerG2p::new(&runner, "pl", Some("/nie/ma.bin".into()), None)).contains("brak wag"));
}

#[test]
fn runner_parallel_error_and_cancel() {
    let tmp = tempfile::tempdir().unwrap();
    let (runner, _) = fake_runner(tmp.path());
    let g = RunnerG2p::new(&runner, "pl", None, Some(8)).unwrap();
    let c = CancelToken::new();

    let texts: Vec<String> = (0..8).map(|i| format!("SLOW zdanie {i}")).collect(); // każdy czeka 0,3 s
    let start = Instant::now();
    let out = g.phonemize_many(&texts, &c).unwrap();
    let dt = start.elapsed();
    for (i, o) in out.iter().enumerate() {
        assert_eq!(o, &format!("slow zdanie {i}"), "kolejność wyników zachowana");
        // tr: SLOW -> slow? (tylko A-Z ASCII)
    }
    assert!(dt < Duration::from_millis(1500), "phonemize_many nie jest równoległe: {dt:?} (sekwencyjnie ok. 2,4 s)");

    // błąd jednego elementu przerywa całość
    let r = g.phonemize_many(&["ala".into(), "BOOM".into(), "kot".into()], &c);
    assert!(matches!(r, Err(Error::Runner(_))), "{r:?}");

    // anulowanie zabija podprocesy (HANG = exec sleep 30) i wraca szybko
    let hang: Vec<String> = (0..8).map(|i| format!("HANG {i}")).collect();
    let token = CancelToken::new();
    let t2 = token.clone();
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        t2.cancel();
    });
    let start = Instant::now();
    let r = g.phonemize_many(&hang, &token);
    canceller.join().unwrap();
    assert!(matches!(r, Err(Error::Cancelled)), "{r:?}");
    assert!(start.elapsed() < Duration::from_secs(2), "anulowanie nie przerwało podprocesów: {:?}", start.elapsed());
}

#[test]
fn runner_language_weights_and_english_extras() {
    let tmp = tempfile::tempdir().unwrap();
    let (runner, _) = fake_runner(tmp.path());
    let c = CancelToken::new();

    // wagi wyprowadzane per język: data/<lang>/phonemizer_<lang z _>.bin
    let de = fake_weights(tmp.path(), "de");
    let g = RunnerG2p::new(&runner, "de", None, None).unwrap();
    assert_eq!((g.lang(), g.weights()), ("de", de.as_path()));
    assert_eq!(g.phonemize("ARGS", &c).unwrap(), format!("--lang de --model {}", de.display()));

    // angielski: leksykon i tagger leżące obok wag są przekazywane runnerowi
    let en = fake_weights(tmp.path(), "en-us");
    assert!(en.ends_with("data/en-us/phonemizer_en_us.bin"), "{}", en.display());
    let dir = en.parent().unwrap();
    std::fs::write(dir.join("lexicon_full.json"), "{}").unwrap();
    std::fs::write(dir.join("tagger.json"), "{}").unwrap();
    let g = RunnerG2p::new(&runner, "en-us", None, None).unwrap();
    assert_eq!(
        g.phonemize("ARGS", &c).unwrap(),
        format!(
            "--lang en-us --model {} --lexicon {} --tagger {}",
            en.display(),
            dir.join("lexicon_full.json").display(),
            dir.join("tagger.json").display()
        )
    );

    let err = |r: plkokoro::Result<RunnerG2p>| r.err().map(|e| e.to_string()).unwrap_or_default();
    // język bez wag w repo -> czytelny błąd o wagach; język spoza Phonemis -> lista dostępnych
    assert!(err(RunnerG2p::new(&runner, "fr", None, None)).contains("data/fr/phonemizer_fr.bin"));
    let m = err(RunnerG2p::new(&runner, "ja", None, None));
    assert!(m.contains("nie obsługuje języka \"ja\"") && m.contains("en-us"), "{m}");
    assert!(err(RunnerG2p::new(&runner, "../pl", None, None)).contains("nie obsługuje"));
}
