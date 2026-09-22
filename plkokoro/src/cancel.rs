use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::error::{Error, Result};

/// Współdzielony znacznik anulowania. Klon wskazuje ten sam stan: wywołaj `cancel()` z dowolnego wątku
/// (np. z obsługi Ctrl+C), a trwająca fonemizacja zabije podprocesy, synteza zatrzyma się między porcjami,
/// i obie zwrócą `Error::Cancelled`. Pojedynczej inferencji ONNX ani pobierania pliku nie da się przerwać.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// `Err(Error::Cancelled)`, jeśli anulowano.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}
