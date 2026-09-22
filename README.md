# plkokoro-rs

Polski TTS w Rust: **Kokoro** (ONNX) + **Phonemis** (tekst → IPA). Biblioteka (`plkokoro`) ma cztery operacje —
ładowanie modelu, tekst → IPA, IPA → audio, zwolnienie — a CLI (`plkokoro-cli`) to cienka, osobna warstwa nad nimi.
Port wersji Pythona i Go; wspólny układ plików modelu i korpus zgodności (15 565 przypadków).

```rust
let model = plkokoro::load_model(plkokoro::Config::default())?;      // PHONEMIS_RUNNER, ORT_LIBRARY_PATH z env
let ipa = model.text_to_ipa("Mam 123 zł, 5% rabatu.", &Default::default())?;
let wav = model.synthesize(&ipa, &plkokoro::SynthOptions::default())?;
plkokoro::write_wav("out.wav", &wav)?;
```

```fish
devenv shell && pk-phonemis-build && pk-fetch-model && pk-doctor
cargo run --release -p plkokoro-cli -- "Cześć, to jest test." -o out.wav
pk-test
```

**Dokumentacja: [docs/README.md](docs/README.md)** (działanie, API, CLI, devenv, procedura testów).

Stan: 43 testy przechodzą (kod wyjścia 0) na ONNX Runtime 1.21.0 / 1.22.0 / 1.23.2 / 1.30.0. **Niesprawdzone**: prawdziwe
wagi Phonemis i model Kokoro (akceptacja A1–A9), providery GPU, TLS, `devenv shell`, pamięć po `unload` z prawdziwym
modelem — patrz [docs/testy.md](docs/testy.md#czego-testy-nie-pokrywają). Znana różnica względem Go: `unload` nie oddaje
pamięci samej biblioteki ORT ([docs/dzialanie.md](docs/dzialanie.md), §6).
