# plkokoro-rs — dokumentacja

Polski TTS w Rust: **Kokoro** (model ONNX, `Shusek00/kokoro-kmp-models`) + **Phonemis** (tekst → IPA).
Biblioteka ma cztery operacje (ładowanie modelu, tekst → IPA, IPA → audio, zwolnienie pamięci), a CLI to cienka,
osobna warstwa nad nimi. Port wersji Pythona (`pl_kokoro_tts.py`) i Go (`plkokoro`); wszystkie trzy mają ten sam układ
plików modelu i ten sam korpus zgodności.

## Spis dokumentów

| Plik | Po co |
|---|---|
| [dzialanie.md](dzialanie.md) | jak to działa w środku: przepływ danych, formaty, model lokalnie, cykl życia i pamięć, decyzje, ograniczenia |
| [api.md](api.md) | referencja biblioteki: `Config`, cztery operacje, wybór języka mówcy / głosu / języka fonemizera, opcje, błędy, własny backend `G2p`, cechy Cargo |
| [cli.md](cli.md) | referencja CLI: opcje, kody wyjścia, logi, przykłady |
| [devenv.md](devenv.md) | Nix: devenv, moduł dla innych projektów (`devenv.yaml` / flake), `nix run`, pakiety (Phonemis, model Kokoro, CLI), opcje `plkokoro.voices` / `plkokoro.phonemisLanguages`, polecenia `pk-*`, podbijanie wersji |
| [testy.md](testy.md) | **pełna procedura testów**: poziomy, polecenia, oczekiwane wyniki, diagnostyka, czego testy nie pokrywają |
| `../plkokoro/examples/` | uruchamialne przykłady (`basic`, `custom_g2p`, `memory`), kompilowane przez `cargo build --examples` |

## Szybki start

```fish
devenv shell                      # Rust, ORT, Phonemis i model Kokoro jako pakiety Nix (pierwsze wejście buduje/pobiera)
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
nix/                    devenv.nix (moduł devenv), modules/ (opcje), pkgs/ (pakiety Nix), catalog.json (kopia katalogu v2.1.1)
flake.nix               wyjścia flake: CLI, pakiety, lib.mk*, devenvModules, overlays
testdata/               parity.json (referencja z Pythona), tiny_kokoro.onnx (atrapa modelu), generatory
docs/                   ta dokumentacja
devenv.nix, devenv.yaml środowisko devenv
```

## Wymagania w czasie działania

- Rust >= 1.88 (MSRV crate'a `ort`) i kompilator C (zależność `ring` w `ureq`/`rustls`). Testowano na rustc 1.91.1.
- `libonnxruntime.so` **>= 1.21** (`Config::ort_library` / `ORT_LIBRARY_PATH`), ładowana dynamicznie — nic nie jest
  linkowane w czasie budowania. Sprawdzone: 1.21.0, 1.22.0, 1.23.2, 1.30.0 (Linux x86_64).
- `phonemis_runner` i wagi języka fonemizera (`data/<język>/phonemizer_<język>.bin`) — w devenv pakiet `nix/pkgs/phonemis.nix`
  ([devenv.md](devenv.md)); poza nim zbuduj Phonemis (CMake, `-DBUILD_RUNNER=ON`) i pobierz wagi z Git LFS.
- Model Kokoro pobierany raz z Hugging Face do lokalnego katalogu (rozmiar zależy od wariantu w repo HF; nie
  mierzyłem prawdziwego pliku).
