//! Magazyn modelu (przez `download_model`): lokalnie najpierw, tryb offline, pusty plik, błędy HTTP, ucięta odpowiedź,
//! ścieżki spoza katalogu. Serwer HF jest lokalny (tiny_http), więc sieć nie ma znaczenia.

mod common;

use std::path::Path;

use common::*;
use plkokoro::{download_model, Config, Error, REVISION};
use tiny_http::Response;

fn cfg(dir: &Path, url: &str) -> Config {
    Config { model_dir: Some(dir.to_path_buf()), hf_endpoint: Some(url.to_string()), ..Default::default() }
}

fn files_under(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p.display().to_string());
            }
        }
    }
    out
}

#[test]
fn local_first_offline_and_fetch() {
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("m");

    let paths = download_model(&cfg(&dir, &hf.url)).unwrap();
    assert_eq!(paths.len(), 4);
    for p in &paths {
        assert!(std::fs::metadata(p).is_ok_and(|m| m.len() > 0), "brak pliku {}", p.display());
        assert!(p.starts_with(dir.join(REVISION)), "plik poza <dir>/<rewizja>: {}", p.display());
    }
    assert!(files_under(&dir).iter().all(|f| !f.ends_with(".part")), "zostały pliki .part");

    let before = hf.count();
    download_model(&cfg(&dir, &hf.url)).unwrap();
    assert_eq!(hf.count(), before, "drugi raz użył sieci");

    // offline z lokalną kopią działa, bez niej daje czytelny błąd
    download_model(&Config { offline: true, ..cfg(&dir, &hf.url) }).expect("offline z kopią");
    let err = download_model(&Config { offline: true, ..cfg(&tmp.path().join("pusty"), &hf.url) }).unwrap_err();
    assert!(err.to_string().contains("brak lokalnego pliku"), "{err}");
}

#[test]
fn empty_file_is_redownloaded() {
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    download_model(&cfg(tmp.path(), &hf.url)).unwrap();
    let f = tmp.path().join(REVISION).join("cfg.json");
    std::fs::write(&f, b"").unwrap(); // przerwane pobieranie zostawiło pusty plik
    download_model(&cfg(tmp.path(), &hf.url)).unwrap();
    assert!(std::fs::metadata(&f).unwrap().len() > 0, "pusty plik nie został pobrany ponownie");
}

#[test]
fn http_errors_and_truncation() {
    // 404 na wszystko
    let srv404 = TestServer::start(|req| drop(req.respond(Response::empty(404))));
    let tmp = tempfile::tempdir().unwrap();
    let err = download_model(&cfg(tmp.path(), &srv404.url)).unwrap_err();
    assert!(matches!(err, Error::Http(_)) && err.to_string().contains("HTTP 404"), "{err}");

    // ucięta odpowiedź: Content-Length 1000, treść 6 bajtów, zamknięcie połączenia -> błąd i brak plików na dysku
    let url = raw_server(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nConnection: close\r\n\r\nkrotko".to_vec());
    let dir = tempfile::tempdir().unwrap();
    assert!(download_model(&cfg(dir.path(), &url)).is_err(), "ucięta odpowiedź powinna dać błąd");
    assert!(files_under(dir.path()).is_empty(), "po błędzie zostały pliki: {:?}", files_under(dir.path()));
}

#[test]
fn rejects_path_traversal_from_catalog() {
    for bad in ["../evil", "a/../../evil", "/etc/passwd"] {
        let mut files = default_files();
        files[0] = ("catalog.json", catalog_json(bad).into_bytes());
        let hf = fake_hf_with(files);
        let tmp = tempfile::tempdir().unwrap();
        let err = download_model(&cfg(tmp.path(), &hf.url)).unwrap_err();
        assert!(err.to_string().contains("niebezpieczna"), "{bad:?}: {err}");
    }
}

#[test]
fn catalog_without_polish_language() {
    let mut files = default_files();
    files[0] = (
        "catalog.json",
        br#"{"runtime":{"tokenEncoding":{"vocabularyField":"vocab"}},"languages":[],"models":[],"tokenizers":[]}"#.to_vec(),
    );
    let hf = fake_hf_with(files);
    let tmp = tempfile::tempdir().unwrap();
    let err = download_model(&cfg(tmp.path(), &hf.url)).unwrap_err();
    assert!(err.to_string().contains("nie znaleziono języka"), "{err}");
}

#[test]
fn cancelled_download_leaves_nothing() {
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    let cancel = plkokoro::CancelToken::new();
    cancel.cancel();
    let err = download_model(&Config { cancel: Some(cancel), ..cfg(tmp.path(), &hf.url) }).unwrap_err();
    assert!(matches!(err, Error::Cancelled), "{err}");
    assert_eq!(hf.count(), 0, "anulowane pobieranie użyło sieci");
    assert!(files_under(tmp.path()).is_empty());
}
