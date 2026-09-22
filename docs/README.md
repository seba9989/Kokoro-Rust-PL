# plkokoro-rs — dokumentacja

Polski TTS w Rust: **Kokoro** (model ONNX, `Shusek00/kokoro-kmp-models`) + **Phonemis** (tekst → IPA).
Biblioteka ma cztery operacje (ładowanie modelu, tekst → IPA, IPA → audio, zwolnienie pamięci), a CLI to cienka,
osobna warstwa nad nimi. Port wersji Pythona (`pl_kokoro_tts.py`) i Go (`plkokoro`); wszystkie trzy mają ten sam układ
plików modelu i ten sam korpus zgodności.

## Spis dokumentów

| Plik | Po co |
|---|---|
| [dzialanie.md](dzialanie.md) | jak to działa w środku: przepływ danych, formaty, model lokalnie, cykl życia i pamięć, decyzje, ograniczenia |
| [api.md](api.md) | referencja biblioteki: `Config`, cztery operacje, opcje, błędy, własny backend `G2p`, cechy Cargo |
| [cli.md](cli.md) | referencja CLI: opcje, kody wyjścia, logi, przykłady |
| [phonemis-build.md](phonemis-build.md) | budowanie `phonemis_runner` w katalogu tymczasowym i instalacja minimalnego zestawu (wspólne z wersją Go) |
| [devenv.md](devenv.md) | środowisko devenv/Nix: co ustawia, polecenia `pk-*`, nadpisywanie |
| [testy.md](testy.md) | **pełna procedura testów**: poziomy, polecenia, oczekiwane wyniki, diagnostyka, czego testy nie pokrywają |
| `../plkokoro/examples/` | uruchamialne przykłady (`basic`, `custom_g2p`, `memory`), kompilowane przez `cargo build --examples` |

## Szybki start

```fish
devenv shell                      # Rust, ORT z nixpkgs, zmienne środowiskowe
pk-phonemis-build                 # runner + wagi -> ./phonemis (build w /tmp)
pk-fetch-model                    # model Kokoro -> $KOKORO_MODEL_DIR (raz)
pk-doctor                         # czy wszystko jest na miejscu
cargo run --release -p plkokoro-cli -- "Cześć, to jest test." -o out.wav
pk-test                           # cargo test --workspace
```

Bez devenv: `cargo build --release -p plkokoro-cli`, potem ustaw `ORT_LIBRARY_PATH`, `PHONEMIS_RUNNER`
(i opcjonalnie `KOKORO_MODEL_DIR`) — patrz [devenv.md](devenv.md).

## Mapa katalogów

```
Cargo.toml              workspace (członkowie: plkokoro, plkokoro-cli)
plkokoro/               BIBLIOTEKA
  src/lib.rs              Config, load_model, Model (text_to_ipa, synthesize, unload), download_model
  src/g2p.rs              trait G2p i backend phonemis_runner (podproces, równolegle, anulowanie)
  src/store.rs            lokalna kopia modelu, pobieranie z HF (ureq), catalog.json, słownik, styl głosu
  src/engine.rs           sesja ONNX Runtime (crate `ort`, ładowanie dynamiczne), providery
  src/normalize.rs        normalizacja uzupełniająca (zł, %, skróty, godziny, cudzysłowy)
  src/overrides.rs        ręczne fonemy [tekst](/ipa/)
  src/text.rs             podział na akapity/zdania, cięcie IPA na porcje (bez lookaroundów)
  src/wav.rs, cancel.rs, error.rs, trim.rs
  tests/                  parity, g2p, store, model, wav_engine + common/ (fixtures)
  examples/               basic, custom_g2p, memory
plkokoro-cli/           CLI (src/main.rs) i jego testy end-to-end (tests/cli.rs)
scripts/                phonemis-build.sh (budowanie Phonemis), test-phonemis-build.sh (jego testy)
testdata/               parity.json (referencja z Pythona), tiny_kokoro.onnx (atrapa modelu), generatory
docs/                   ta dokumentacja
devenv.nix, devenv.yaml środowisko devenv
```

## Wymagania w czasie działania

- Rust >= 1.88 (MSRV crate'a `ort`) i kompilator C (zależność `ring` w `ureq`/`rustls`). Testowano na rustc 1.91.1.
- `libonnxruntime.so` **>= 1.21** (`Config::ort_library` / `ORT_LIBRARY_PATH`), ładowana dynamicznie — nic nie jest
  linkowane w czasie budowania. Sprawdzone: 1.21.0, 1.22.0, 1.23.2, 1.30.0 (Linux x86_64).
- `phonemis_runner` i wagi `phonemizer_pl.bin` (Git LFS) — patrz [phonemis-build.md](phonemis-build.md).
- Model Kokoro pobierany raz z Hugging Face do lokalnego katalogu (rozmiar zależy od wariantu w repo HF; nie
  mierzyłem prawdziwego pliku).
