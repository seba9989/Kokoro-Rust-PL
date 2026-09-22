//! Własny backend G2P (trait `plkokoro::G2p`): zamiast `phonemis_runner` biblioteka używa Twojego kodu.
//! Tu „fonemizacja" to tylko zamiana na małe litery — służy pokazaniu kontraktu. Nic nie wymaga ORT ani modelu
//! (`skip_kokoro`), więc uruchomisz to od razu:
//!
//!     cargo run -p plkokoro --example custom_g2p

use plkokoro::{load_model, CancelToken, Config, G2p, TextOptions};

struct Lower;

impl G2p for Lower {
    fn phonemize(&self, text: &str, _cancel: &CancelToken) -> plkokoro::Result<String> {
        Ok(text.to_lowercase())
    }
    // phonemize_many (równolegle) i close() mają sensowne domyślne implementacje
}

fn main() -> plkokoro::Result<()> {
    let model = load_model(Config { g2p: Some(Box::new(Lower)), skip_kokoro: true, ..Default::default() })?;
    // Wstawka [Kokoro](/kɔkˈɔrɔ/) omija backend: dostaje on tylko zwykły tekst.
    let ipa = model.text_to_ipa("Ala MA kota. Lubię [Kokoro](/kɔkˈɔrɔ/).", &TextOptions::default())?;
    println!("{ipa}");
    Ok(())
}
