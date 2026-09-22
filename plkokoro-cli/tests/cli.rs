//! Testy end-to-end CLI: prawdziwa binarka `plkokoro` jako podproces, fałszywy serwer HF, fałszywy runner i (dla
//! syntezy) prawdziwy ONNX Runtime z modelem-atrapą. Fixtures są wspólne z testami biblioteki.
//! Bez ORT_LIBRARY_PATH testy z sesją ONNX wypisują SKIP i kończą się od razu.

#[path = "../../plkokoro/tests/common/mod.rs"]
mod common;

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use common::*;

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Uruchamia binarkę w izolowanym środowisku (bez zmiennych z maszyny deweloperskiej), z limitem czasu.
fn cli(args: &[&str], env: &[(&str, &str)]) -> Out {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_plkokoro"));
    cmd.args(args).env_clear();
    // fałszywy runner to skrypt bash używający tr/sleep: potrzebny PATH
    cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("start");
    let start = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            panic!("CLI nie zakończyło się w 60 s: {args:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let o = child.wait_with_output().unwrap();
    Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into(),
        stderr: String::from_utf8_lossy(&o.stderr).into(),
    }
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// Rozmiar sekcji `data` i nagłówek WAV: (sample_rate, kanały, bity, bajty_danych).
fn wav_info(path: &Path) -> (u32, u16, u16, u32) {
    let b = std::fs::read(path).expect("brak pliku wyjściowego");
    assert_eq!((&b[0..4], &b[8..12], &b[36..40]), (&b"RIFF"[..], &b"WAVE"[..], &b"data"[..]));
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let u16_at = |o: usize| u16::from_le_bytes(b[o..o + 2].try_into().unwrap());
    assert_eq!(b.len() as u32, 44 + u32_at(40));
    (u32_at(24), u16_at(22), u16_at(34), u32_at(40))
}

#[test]
fn fetch_then_offline() {
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("m");
    let env = [("HF_ENDPOINT", hf.url.as_str())];
    let r = cli(&["--fetch-only", "--model-dir", s(&dir)], &env);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.stdout.lines().count(), 4, "{}", r.stdout);
    assert!(r.stdout.lines().all(|l| l.contains(" MB)")), "{}", r.stdout);

    let before = hf.count();
    let r = cli(&["--fetch-only", "--model-dir", s(&dir)], &env);
    assert_eq!((r.code, hf.count()), (0, before), "drugi raz użył sieci: {}", r.stderr);
    drop(hf); // serwera już nie ma: --offline i kopia lokalna muszą wystarczyć
    let r = cli(&["--fetch-only", "--offline", "--model-dir", s(&dir)], &[("HF_ENDPOINT", "http://127.0.0.1:1")]);
    assert_eq!(r.code, 0, "{}", r.stderr);
}

#[test]
fn phonemize_only_no_kokoro() {
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    let (runner, _) = fake_runner(tmp.path());
    let r = cli(&["--phonemize-only", "--runner", s(&runner), "Mam", "123 zł,", "5% rabatu."], &[("HF_ENDPOINT", hf.url.as_str())]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.stdout.trim(), "mam 123 złote, 5 procent rabatu.");
    assert!(r.stderr.contains("[normalize]"), "--phonemize-only włącza logi debug: {}", r.stderr);
    assert_eq!(hf.count(), 0, "--phonemize-only użył sieci");

    // --no-normalize zostawia tekst bez rozwinięć
    let r = cli(&["--phonemize-only", "--no-normalize", "--runner", s(&runner), "5% rabatu."], &[]);
    assert_eq!((r.code, r.stdout.trim()), (0, "5% rabatu."), "{}", r.stderr);

    // runner ze zmiennej środowiskowej
    let r = cli(&["--phonemize-only", "Ala"], &[("PHONEMIS_RUNNER", s(&runner))]);
    assert_eq!((r.code, r.stdout.trim()), (0, "ala"), "{}", r.stderr);
}

#[test]
fn synthesize_wav_flags_after_text() {
    let ort = require_ort!();
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    let (runner, _) = fake_runner(tmp.path());
    let (dir, out) = (tmp.path().join("m"), tmp.path().join("o.wav"));
    let env = [("HF_ENDPOINT", hf.url.as_str())];
    // flagi PO tekście
    let args = ["Ala ma kota.", "-o", s(&out), "--model-dir", s(&dir), "--runner", s(&runner), "--ort-lib", s(&ort), "--speed", "1.25"];
    let r = cli(&args, &env);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.contains("Zapisano") && r.stdout.contains(s(&out)), "{}", r.stdout);
    // "ala ma kota." = 12 znaków -> (12+2)*100 próbek modelu-atrapy, po 2 bajty
    assert_eq!(wav_info(&out), (24000, 1, 16, 14 * 100 * 2));

    // po pobraniu sieć nie jest potrzebna
    drop(hf);
    let r = cli(&args, &[("HF_ENDPOINT", "http://127.0.0.1:1"), ("KOKORO_OFFLINE", "1")]);
    assert_eq!(r.code, 0, "{}", r.stderr);
}

#[test]
fn ipa_and_file_input() {
    let ort = require_ort!();
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    let (runner, _) = fake_runner(tmp.path());
    let dir = tmp.path().join("m");
    let out = tmp.path().join("o.wav");
    let base = |extra: &[&str]| -> Vec<String> {
        let mut v: Vec<String> =
            ["-o", s(&out), "--model-dir", s(&dir), "--runner", s(&runner), "--ort-lib", s(&ort)].iter().map(|x| x.to_string()).collect();
        v.extend(extra.iter().map(|x| x.to_string()));
        v
    };
    let run = |args: Vec<String>| cli(&args.iter().map(String::as_str).collect::<Vec<_>>(), &[("HF_ENDPOINT", hf.url.as_str())]);

    let r = run(base(&["--ipa", "ala ma"])); // 6 znaków
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(wav_info(&out).3, (6 + 2) * 100 * 2);

    let file = tmp.path().join("t.txt");
    std::fs::write(&file, "Ala ma kota.\n").unwrap();
    let r = run(base(&["-f", s(&file)]));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(wav_info(&out).3, 14 * 100 * 2);

    // wejście złożone wyłącznie ze znaków spoza słownika
    let r = run(base(&["--ipa", "@@@"]));
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("brak fonemów"), "{}", r.stderr);
}

#[test]
fn errors_and_exit_codes() {
    let tmp = tempfile::tempdir().unwrap();
    let (runner, _) = fake_runner(tmp.path());
    let dir = tmp.path().join("pusty");
    let hf = fake_hf();
    let (r_s, d_s) = (s(&runner).to_string(), s(&dir).to_string());
    let missing = tmp.path().join("nie-ma.txt");
    let m_s = s(&missing).to_string();

    struct Case(&'static str, Vec<String>, i32, &'static str);
    let v = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let cases = vec![
        Case("nieznana flaga", v(&["--nieznana"]), 2, "unexpected argument"),
        Case("pomoc", v(&["--help"]), 0, ""),
        Case("brak runnera", v(&["--phonemize-only", "ala"]), 1, "nie podano Phonemis"),
        Case("runner nie istnieje", v(&["--phonemize-only", "--runner", "/nie/ma", "ala"]), 1, "brak phonemis_runner"),
        Case("plik wejściowy nie istnieje", v(&["--phonemize-only", "--runner", &r_s, "-f", &m_s]), 1, "nie-ma.txt"),
        Case("offline bez kopii", v(&["--offline", "--model-dir", &d_s, "--runner", &r_s, "ala"]), 1, "brak lokalnego pliku modelu"),
        Case(
            "zły provider",
            v(&["--providers", "FooExecutionProvider", "--model-dir", &d_s, "--runner", &r_s, "ala"]),
            1,
            "nie jest dostępny",
        ),
        Case("speed 0", v(&["--speed", "0", "--model-dir", &d_s, "--runner", &r_s, "ala"]), 1, ""),
    ];
    for Case(name, args, code, want) in cases {
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        let r = cli(&a, &[("HF_ENDPOINT", hf.url.as_str())]);
        assert_eq!(r.code, code, "{name}: kod, stderr: {}", r.stderr);
        assert!(r.stderr.contains(want) || r.stdout.contains(want), "{name}: oczekiwano {want:?} w {:?}", r.stderr);
    }
    // KOKORO_OFFLINE ze środowiska działa jak --offline
    let fresh = tmp.path().join("swiezy"); // (poprzednie przypadki mogły pobrać model do „pusty")
    let r = cli(&["--model-dir", s(&fresh), "--runner", &r_s, "ala"], &[("KOKORO_OFFLINE", "1"), ("HF_ENDPOINT", hf.url.as_str())]);
    assert!(r.code == 1 && r.stderr.contains("brak lokalnego pliku modelu"), "{}", r.stderr);
}

/// Ctrl+C (SIGINT) w trakcie fonemizacji: CLI zabija podproces runnera i kończy się kodem 1 z komunikatem o
/// anulowaniu (w wersji Go sprawdzane tylko ręcznie).
#[test]
fn sigint_cancels_running_phonemization() {
    let tmp = tempfile::tempdir().unwrap();
    let (runner, _) = fake_runner(tmp.path());
    let mut child = Command::new(env!("CARGO_BIN_EXE_plkokoro"))
        .args(["--phonemize-only", "--runner", s(&runner), "HANG"]) // HANG = runner wisi 30 s
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(600)); // niech runner wystartuje
    let start = Instant::now();
    assert!(Command::new("kill").args(["-INT", &child.id().to_string()]).status().unwrap().success());
    let wait_deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(st) = child.try_wait().unwrap() {
            break st;
        }
        assert!(Instant::now() < wait_deadline, "CLI nie zareagowało na SIGINT w 5 s");
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut err = String::new();
    std::io::Read::read_to_string(&mut child.stderr.take().unwrap(), &mut err).unwrap();
    assert_eq!(status.code(), Some(1), "{err}");
    assert!(err.contains("anulowano"), "{err}");
    assert!(start.elapsed() < Duration::from_secs(3), "zbyt wolne przerwanie: {:?}", start.elapsed());
}
