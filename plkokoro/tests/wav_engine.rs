//! Enkoder WAV oraz silnik: odrzucanie złego modelu, niewspieranych providerów i złej biblioteki ORT.

mod common;

use common::*;
use plkokoro::{encode_wav, load_model, Config, Error};

#[test]
fn encode_wav_header_and_samples() {
    let mut buf = Vec::new();
    encode_wav(&mut buf, &[0.0, 1.0, -1.0, 0.5, 2.0, -3.0], 24000).unwrap(); // 2 i -3 są obcinane
    assert_eq!(buf.len(), 44 + 12);
    assert_eq!(&buf[0..4], b"RIFF");
    assert_eq!(&buf[8..16], b"WAVEfmt ");
    assert_eq!(&buf[36..40], b"data");
    let u32_at = |o: usize| u32::from_le_bytes(buf[o..o + 4].try_into().unwrap());
    let u16_at = |o: usize| u16::from_le_bytes(buf[o..o + 2].try_into().unwrap());
    assert_eq!(u32_at(4) as usize, buf.len() - 8);
    assert_eq!((u32_at(40), u32_at(24), u16_at(22), u16_at(34)), (12, 24000, 1, 16));
    let got: Vec<i16> = buf[44..].chunks(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    assert_eq!(got, vec![0, 32767, -32767, 16384, 32767, -32767]);
}

#[test]
fn engine_rejects_model_with_wrong_io() {
    let ort = require_ort!();
    let mut files = default_files();
    files[3] = ("onnx/model.onnx", read_testdata("bad_io.onnx"));
    let hf = fake_hf_with(files);
    let tmp = tempfile::tempdir().unwrap();
    let err = load_model(base_cfg(tmp.path(), &hf.url, &ort)).err().expect("zły model powinien być odrzucony");
    let m = err.to_string();
    assert!(matches!(err, Error::Ort(_)) && m.contains("inne I/O") && m.contains("input_ids"), "{m}");
}

#[test]
fn unsupported_provider_is_rejected() {
    // ROCm/MIGraphX i inne wymagają cechy Cargo; bez niej nazwa jest odrzucana (jeszcze przed inicjalizacją ORT)
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    let cfg =
        Config { providers: vec!["FooExecutionProvider".into()], ..base_cfg(tmp.path(), &hf.url, std::path::Path::new("/nie/ma.so")) };
    let err = load_model(cfg).err().expect("nieznany provider");
    assert!(err.to_string().contains("nie jest dostępny"), "{err}");
}

#[test]
fn wrong_ort_library_is_reported() {
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    let cfg = base_cfg(tmp.path(), &hf.url, std::path::Path::new("/nie/ma/libonnxruntime.so"));
    let err = load_model(cfg).err().expect("zła biblioteka");
    assert!(err.to_string().contains("libonnxruntime"), "{err}");
}

/// Provider poza CPU, którego załadowana biblioteka ORT nie ma (tu: ORT z CPU, bez MIGraphX), musi dać błąd, a nie
/// po cichu wrócić na CPU (rejestrujemy z `error_on_failure`). Uruchom: `cargo test --features migraphx`.
#[cfg(feature = "migraphx")]
#[test]
fn unavailable_provider_fails_loudly() {
    let ort = require_ort!();
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    let cfg = Config { providers: vec!["MIGraphXExecutionProvider".into()], ..base_cfg(tmp.path(), &hf.url, &ort) };
    let err = load_model(cfg).err().expect("MIGraphX niedostępny w tej bibliotece ORT — oczekiwano błędu, a nie cichego CPU");
    assert!(matches!(err, Error::Ort(_)), "{err}");
    eprintln!("komunikat: {err}");
}
