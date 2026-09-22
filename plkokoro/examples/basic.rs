//! Podstawowy cykl: załaduj model, zamień tekst na IPA, zsyntezuj, zapisz WAV, zwolnij model.
//!
//! Wymaga: phonemis_runner (PHONEMIS_RUNNER), libonnxruntime.so (ORT_LIBRARY_PATH) i modelu Kokoro (pobierze się
//! sam do KOKORO_MODEL_DIR przy pierwszym uruchomieniu).
//!
//!     cargo run --release -p plkokoro --example basic -- "Cześć, to jest test."

use plkokoro::{load_model, write_wav, Config, SynthOptions, TextOptions, SAMPLE_RATE};

fn main() -> plkokoro::Result<()> {
    let text = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    let text = if text.is_empty() { "Cześć, to jest test polskiej syntezy mowy.".to_string() } else { text };

    let model = load_model(Config::default())?; // runner, ORT i katalog modelu ze zmiennych środowiskowych
    let ipa = model.text_to_ipa(&text, &TextOptions::default())?;
    println!("IPA: {ipa}");

    // Zmieniasz tylko tempo — pauzy zostają domyślne dzięki ..Default::default()
    let wav = model.synthesize(&ipa, &SynthOptions { speed: 1.1, ..Default::default() })?;
    write_wav("basic.wav", &wav)?;
    println!("basic.wav: {:.1} s", wav.len() as f64 / f64::from(SAMPLE_RATE));

    model.unload() // opcjonalne: Drop też zwalnia model
}
