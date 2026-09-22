use std::fmt;

/// Wynik operacji biblioteki.
pub type Result<T> = std::result::Result<T, Error>;

/// Błędy biblioteki. Warianty `Unloaded`, `NoKokoro`, `NoPhonemes` i `Cancelled` służą do dopasowywania
/// w kodzie wywołującym (`matches!(err, Error::Unloaded)`); pozostałe niosą opisowy komunikat z podpowiedzią naprawy.
#[derive(Debug)]
pub enum Error {
    /// Model został zwolniony (`Model::unload`) — załaduj go ponownie przez `load_model`.
    Unloaded,
    /// Model załadowano z `Config::skip_kokoro` — dostępne tylko `text_to_ipa`.
    NoKokoro,
    /// Nie ma czego syntezować (puste wejście albo same znaki spoza słownika Kokoro).
    NoPhonemes,
    /// Operacja została przerwana przez `CancelToken`.
    Cancelled,
    /// Niepoprawna opcja (np. `speed <= 0`).
    InvalidOption(String),
    /// Błąd konfiguracji: brak runnera, wag, biblioteki ORT, pliku modelu w trybie offline itd.
    Config(String),
    Io(std::io::Error),
    Http(String),
    Json(String),
    /// Błąd backendu fonemizacji (np. `phonemis_runner` zakończył się kodem ≠ 0).
    Runner(String),
    /// Błąd ONNX Runtime.
    Ort(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unloaded => f.write_str("plkokoro: model został zwolniony (unload) — załaduj go ponownie przez load_model()"),
            Error::NoKokoro => f.write_str("plkokoro: model załadowano z skip_kokoro — synteza niedostępna, tylko text_to_ipa"),
            Error::NoPhonemes => f.write_str("plkokoro: brak fonemów do syntezy (pusty tekst albo same znaki spoza słownika Kokoro)"),
            Error::Cancelled => f.write_str("plkokoro: operacja przerwana (anulowano)"),
            Error::InvalidOption(m) | Error::Config(m) | Error::Http(m) | Error::Json(m) | Error::Runner(m) | Error::Ort(m) => {
                f.write_str(m)
            }
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e.to_string())
    }
}
