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
devenv shell && pk-doctor          # Phonemis i model Kokoro to pakiety Nix (nix/)
cargo run --release -p plkokoro-cli -- "Cześć, to jest test." -o out.wav
cargo run --release -p plkokoro-cli -- --list-voices                     # 11 języków mówcy, 159 głosów
pk-test -- --test-threads=1
```

Domyślnie wszystko jest polskie, ale język mówcy (`--lang` / `Config::lang`), głos (`--voice`) i język fonemizera
(`--phonemis-lang`) wybiera się niezależnie; w devenv głosy i języki Phonemis to opcje `plkokoro.voices`
i `plkokoro.phonemisLanguages` ([docs/devenv.md](docs/devenv.md)).

Z innych projektów na devenv: w `devenv.yaml` input `plkokoro: { url: github:seba9989/Kokoro-Rust-PL, flake: false }`
i `imports: - plkokoro/nix` — ten sam moduł i opcje co tutaj. Jest też `flake.nix`: `plkokoro.lib.mkPlkokoro { pkgs; voices = [ … ]; }`,
`devenvModules.default`, a bez projektu `nix run github:seba9989/Kokoro-Rust-PL -- "Cześć." -o out.wav`.

**Dokumentacja: [docs/README.md](docs/README.md)** (działanie, API, CLI, devenv, procedura testów).

Stan: 53 testy przechodzą uruchamiane po kolei (ORT 1.27.1 z nixpkgs; wcześniej 43 na 1.21.0 / 1.22.0 / 1.23.2 / 1.30.0);
równolegle test `wav_engine` bywa przerywany przez błąd crate'a `ort` ([docs/testy.md](docs/testy.md), poziom 2).
Synteza na prawdziwych danych sprawdzona dla pl i de. **Niesprawdzone**: pozostałe języki odsłuchem, providery GPU, TLS,
pamięć po `unload` z prawdziwym modelem — patrz [docs/testy.md](docs/testy.md#czego-testy-nie-pokrywają). Znana różnica względem Go: `unload` nie oddaje
pamięci samej biblioteki ORT ([docs/dzialanie.md](docs/dzialanie.md), §6).
