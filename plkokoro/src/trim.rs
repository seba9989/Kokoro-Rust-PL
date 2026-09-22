//! Oddanie systemowi pamięci zwolnionej przez ORT (glibc trzyma ją w puli po zniszczeniu sesji).

#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub(crate) fn trim_native() {
    extern "C" {
        fn malloc_trim(pad: usize) -> core::ffi::c_int;
    }
    // SAFETY: malloc_trim(0) jest bezpieczne do wywołania w dowolnym momencie; nie przyjmuje wskaźników.
    unsafe {
        malloc_trim(0);
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub(crate) fn trim_native() {}
