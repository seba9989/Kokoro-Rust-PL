# Środowisko devenv (Nix)

Pliki: `devenv.nix`, `devenv.yaml`, `.gitignore` (w paczce `.zip` z oryginalnymi nazwami).

## Co ustawia `devenv.nix`

| Element | Wartość |
|---|---|
| Rust | `languages.rust.enable` (kanał `nixpkgs`: rustc, cargo, clippy, rustfmt, rust-analyzer). Nowszy toolchain: `languages.rust.channel = "stable"` (wymaga inputu `rust-overlay`) |
| kompilator C | `stdenv` devenv (zależność `ring` w `ureq`/`rustls` buduje C) |
| pakiety | `git`, `git-lfs`, `cmake` (devenv nie dodaje gita do powłoki) |
| `ORT_LIBRARY_PATH` | `libonnxruntime.so` z nixpkgs (`pkgs.onnxruntime`); ładowana dynamicznie, nic nie jest linkowane |
| `PHONEMIS_RUNNER` | `<projekt>/phonemis/build/phonemis_runner` (niski priorytet: `lib.mkDefault`) |
| `KOKORO_MODEL_DIR` | `<projekt>/models/kokoro-kmp-models` (niski priorytet) |
| asercje | ORT ≥ 1.21 i rustc ≥ 1.88 — inaczej wejście do powłoki przerywa się z komunikatem |

## Polecenia

Prefiks `pk-` nie przesłania poleceń powłoki (np. `test`).

| Polecenie | Robi |
|---|---|
| `pk-doctor` | sprawdza: `rustc`/`cargo`, `libonnxruntime`, runner (istnieje, uruchamia się), wagi Phonemis (wskaźnik LFS!), model Kokoro |
| `pk-phonemis-build` | buduje i instaluje runner + wagi lokalnie — [phonemis-build.md](phonemis-build.md) |
| `pk-fetch-model` | `cargo run --release -p plkokoro-cli -- --fetch-only` (pobiera model Kokoro do `$KOKORO_MODEL_DIR`) |
| `pk-build` | `cargo build --release -p plkokoro-cli` i kopia binarki do `bin/plkokoro` |
| `pk-test` | `cargo test --workspace` (dodatkowe argumenty przekazywane, np. `pk-test lifecycle -- --nocapture`) |
| `pk-test-phonemis` | testy skryptu budującego Phonemis — [testy.md](testy.md), poziom 3 |

## Nadpisywanie lokalne

devenv automatycznie wczytuje `devenv.local.nix` (jest w `.gitignore`):

```nix
{ lib, ... }: {
  # inny runner Phonemis
  env.PHONEMIS_RUNNER = "/home/seba9989/Dokumenty/AI/Phonemis/build/phonemis_runner";
  # model Kokoro współdzielony z wersjami Pythona i Go (ten sam układ plików)
  env.KOKORO_MODEL_DIR = "/home/seba9989/Dokumenty/AI/Kokoro/tests/models/kokoro-kmp-models";
}
```

Uwaga (fish): `set NAZWA wartość` bez `-gx` nie eksportuje zmiennej do procesów potomnych — użyj `set -gx`.

## Zweryfikowane i nie

Sprawdzone: `devenv.nix` przechodzi ewaluację przez prawdziwy system modułów devenv (22 asercje, w tym 2 własne) na
nixpkgs-unstable (rustc 1.98.1, ORT 1.27.1); składnia wszystkich skryptów `pk-*` sprawdzona `bash -n`.

**Nie sprawdzono**: prawdziwego `devenv shell` (brak binarnego cache w środowisku, w którym powstało); obecności pliku
`lib/libonnxruntime.so` w zbudowanym pakiecie (wynika z przepisu nixpkgs; `pk-doctor` to sprawdzi); **budowania
projektu na rustc 1.98** (testy szły na rustc 1.91.1) ani z ORT 1.27.1 z nixpkgs (testowano 1.21.0, 1.22.0, 1.23.2,
1.30.0). Jeśli przy pierwszym wejściu `onnxruntime` zaczyna się kompilować ze źródeł zamiast się pobierać, przerwij
i przypnij inną rewizję nixpkgs w `devenv.yaml`.
