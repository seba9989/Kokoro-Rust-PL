# Nix: devenv, moduł dla innych projektów i flake

Wszystkie zależności czasu działania są **deklaratywnymi pakietami Nix**: przypięta rewizja, sumy SHA-256, budowane
i pobierane hermetycznie, cache'owane jak każdy pakiet. Nic nie jest klonowane, kompilowane ani pobierane w
`enterShell`. Ten sam moduł (`nix/devenv.nix`) używa devenv tego repo i dowolny inny projekt na devenv (np. aplikacja
Tauri): wystarczy input w `devenv.yaml` i `imports: - plkokoro/nix`. Dodatkowo `flake.nix` (wzór `cachix/nixpkgs-python`)
wystawia pakiety, funkcje `lib.mk*` i ten sam moduł dla projektów na flake oraz `nix run`.

| Sposób | Dla kogo | Jak |
|---|---|---|
| **moduł devenv** przez `devenv.yaml` | inny projekt na devenv (np. aplikacja Tauri) | input `plkokoro` + `imports: - plkokoro/nix` |
| **flake** | inny projekt na flake | `inputs.plkokoro.devenvModules.default`, `packages`, `lib.mk*`, `overlays.default` |
| **`nix run` / `nix shell`** | bez projektu | gotowe CLI `plkokoro` z modelem i Phonemis |

Układ plików:

```
flake.nix, flake.lock    wyjścia flake (niżej)
nix/devenv.nix           moduł devenv dla innych projektów: importuje modules/, ustawia ORT_LIBRARY_PATH
nix/modules/             opcje devenv: phonemis.nix (plkokoro.phonemisLanguages), kokoro-model.nix (plkokoro.voices)
nix/pkgs/                pakiety (callPackage): phonemis.nix, kokoro-model.nix, plkokoro.nix (CLI), phonemis-files.nix (sumy wag)
nix/catalog.json         kopia catalog.json v2.1.1 z HF (ścieżki i sumy SHA-256 modeli/głosów)
devenv.nix, devenv.yaml  środowisko pracy nad tym repo = nix/devenv.nix + Rust + skrypty pk-*
```

| Pakiet | Źródło | Zawartość |
|---|---|---|
| `pkgs.onnxruntime` (opcja `plkokoro.onnxruntime`) | nixpkgs | `libonnxruntime.so` (ładowana dynamicznie przez crate `ort`) |
| `nix/pkgs/phonemis.nix` | `IgorSwat/Phonemis` @ `71eb1ce` (CMake) + wagi z Git LFS (`fetchurl`, sha256 = `oid` ze wskaźnika LFS) | `build/phonemis_runner` (+ symlink `bin/phonemis_runner`), `data/<język>/phonemizer_<język>.bin` (+ `lexicon_full.json`, `tagger.json` dla `en-us`/`en-gb`); `installCheckPhase` fonemizuje „Test 123.” w każdym języku |
| `nix/pkgs/kokoro-model.nix` | `Shusek00/kokoro-kmp-models` @ `v2.1.1` (`fetchurl`, sumy z `nix/catalog.json`) | `v2.1.1/catalog.json` (prawdziwy, pełny), wybrane głosy, ich modele ONNX i tokenizery — układ `plkokoro::Store` |
| `nix/pkgs/plkokoro.nix` (tylko flake) | to repo (`rustPlatform.buildRustPackage`, `Cargo.lock`) | binarka `plkokoro` owinięta tak, że domyślnie (`--set-default`) wskazuje ORT, Phonemis i model z `/nix/store` |

`nix/catalog.json` jest czytany przy ewaluacji, więc ścieżki i sumy artefaktów pochodzą z niego bez
import-from-derivation.

## Użycie w innym projekcie — devenv

`devenv.yaml`:

```yaml
inputs:
  nixpkgs:
    url: github:cachix/devenv-nixpkgs/rolling
  plkokoro:
    url: github:seba9989/Kokoro-Rust-PL
    flake: false          # moduł potrzebuje tylko źródeł; bez tego devenv ewaluuje też flake.nix (i jego nixpkgs)
imports:
  - plkokoro/nix
```

`devenv.nix` (opcjonalnie — bez tego: głos `pm_mateusz`, Phonemis `pl`):

```nix
{ ... }: {
  plkokoro.voices = [ "pm_mateusz" "df_anna" ];     # Phonemis dostaje de + pl automatycznie
}
```

W powłoce są ustawione `ORT_LIBRARY_PATH`, `PHONEMIS_RUNNER`, `KOKORO_MODEL_DIR` — biblioteka `plkokoro`
(`Config::default()`) znajduje wszystko sama.

## Użycie w innym projekcie — flake

```nix
{
  inputs.plkokoro.url = "github:seba9989/Kokoro-Rust-PL";

  outputs = { self, nixpkgs, plkokoro, ... }: let pkgs = nixpkgs.legacyPackages.x86_64-linux; in {
    # CLI z własnym zestawem głosów (jak lib.mkPython w nixpkgs-python):
    packages.x86_64-linux.tts = plkokoro.lib.mkPlkokoro { inherit pkgs; voices = [ "pm_mateusz" "df_anna" ]; };
    # albo same zależności: plkokoro.lib.mkPhonemis { inherit pkgs; languages = [ "pl" "de" ]; }
    #                       plkokoro.lib.mkKokoroModel { inherit pkgs; voices = [ "af_heart" ]; }
    # moduł devenv (devenv.lib.mkShell): modules = [ plkokoro.devenvModules.default { plkokoro.voices = [ … ]; } ];
  };
}
```

| Wyjście | Zawartość |
|---|---|
| `packages.<system>.{default,plkokoro}` | CLI (głos `pm_mateusz`, Phonemis `pl`) |
| `packages.<system>.{phonemis,kokoro-model}` | same zależności, domyślne zestawy |
| `lib.mkPlkokoro { pkgs; voices; phonemisLanguages ? null; }` | CLI z wybranymi głosami (`null` = języki wynikające z głosów + `pl`) |
| `lib.mkPhonemis { pkgs; languages; }`, `lib.mkKokoroModel { pkgs; voices; }` | pakiety z wybranymi językami / głosami |
| `lib.phonemisLanguages`, `lib.voices` | listy obsługiwanych języków Phonemis i wszystkich id głosów z katalogu |
| `devenvModules.default` | `nix/devenv.nix` |
| `overlays.default` | `plkokoro`, `plkokoro-phonemis`, `plkokoro-kokoro-model` |

## Bez projektu

```fish
nix run github:seba9989/Kokoro-Rust-PL -- "Cześć, to jest test." -o out.wav
nix run github:seba9989/Kokoro-Rust-PL -- --list-voices
nix build github:seba9989/Kokoro-Rust-PL#phonemis
```

## Opcje modułu

W tym repo wpisuj je do `devenv.local.nix` (wczytywany automatycznie, jest w `.gitignore`), w innym projekcie — do
jego `devenv.nix`:

```nix
{ lib, ... }: {
  plkokoro.voices = [ "pm_mateusz" "df_anna" "af_heart" ];   # każdy nowy model to ~325 MB
  plkokoro.phonemisLanguages = [ "pl" "de" "en-us" "fr" ];    # opcjonalnie; domyślnie wynika z głosów
}
```

| Opcja | Domyślnie | Znaczenie |
|---|---|---|
| `plkokoro.voices` | `[ "pm_mateusz" ]` | głosy Kokoro w pakiecie modelu (lista: `plkokoro --list-voices`); nieznany głos przerywa ewaluację z podpowiedzią |
| `plkokoro.phonemisLanguages` | `textFrontend.language` języków wybranych głosów (z katalogu) + `pl` | wagi Phonemis instalowane obok runnera; typ `enum`: `pl en-us en-gb de fr es it pt hi` |
| `plkokoro.onnxruntime` | `pkgs.onnxruntime` | ONNX Runtime (≥ 1.21, asercja) dla `ORT_LIBRARY_PATH` |
| `plkokoro.kokoroModel`, `plkokoro.phonemis` | — (tylko do odczytu) | zbudowane pakiety, np. do użycia w innych modułach |

Zmienne `ORT_LIBRARY_PATH`, `PHONEMIS_RUNNER` (`<phonemis>/bin/phonemis_runner`, symlink — biblioteka rozwiązuje go
i szuka wag obok `build/`), `KOKORO_MODEL_DIR` mają niski priorytet (`lib.mkDefault`), więc można je nadpisać.

Katalog modelu leży w `/nix/store` (tylko do odczytu): głos spoza `plkokoro.voices` daje błąd „katalog … jest tylko do
odczytu — dodaj głos do plkokoro.voices”. Żeby biblioteka pobierała dowolne głosy sama, wskaż katalog z prawem zapisu:
`env.KOKORO_MODEL_DIR = "/home/…/models/kokoro-kmp-models";`.

`KOKORO_LANG` / `KOKORO_VOICE` celowo **nie** są ustawiane (zmieniałyby domyślne zachowanie testów); język i głos
wybieraj flagami CLI (`--lang`, `--voice`) albo w `Config`.

## Co dodaje `devenv.nix` tego repo

| Element | Wartość |
|---|---|
| moduł | `imports = [ ./nix/devenv.nix ]` — ten sam, co w innych projektach |
| Rust | `languages.rust.enable` (kanał `nixpkgs`: rustc, cargo, clippy, rustfmt, rust-analyzer). Nowszy toolchain: `languages.rust.channel = "stable"` (wymaga inputu `rust-overlay`) |
| kompilator C | `stdenv` devenv (zależność `ring` w `ureq`/`rustls` buduje C) |
| pakiety | `git` |
| asercje | rustc ≥ 1.88 (MSRV crate'a `ort`); ORT ≥ 1.21 jest w module |

## Polecenia

Prefiks `pk-` nie przesłania poleceń powłoki (np. `test`).

| Polecenie | Robi |
|---|---|
| `pk-doctor` | sprawdza: `rustc`/`cargo`, `libonnxruntime`, runner (istnieje, uruchamia się), wagi każdego języka z `plkokoro.phonemisLanguages`, model Kokoro; wypisuje głosy obecne w `KOKORO_MODEL_DIR` |
| `pk-build` | `cargo build --release -p plkokoro-cli` i kopia binarki do `bin/plkokoro` |
| `pk-test` | `cargo test --workspace` (dodatkowe argumenty przekazywane, np. `pk-test -- --test-threads=1`) |

Dawne `pk-phonemis-build`, `pk-fetch-model` i `pk-test-phonemis` (skrypt budujący w `/tmp`) zastąpiły pakiety Nix.
Katalogi `phonemis/` i `models/` w projekcie, jeśli zostały z tamtego układu, nie są już używane.

## Podbijanie wersji

- **Phonemis**: nowa rewizja w `nix/pkgs/phonemis.nix` (`rev`), suma źródeł z
  `nix-prefetch-url --unpack https://github.com/IgorSwat/Phonemis/archive/<rev>.tar.gz`, a sumy wag w
  `nix/pkgs/phonemis-files.nix` — pola `oid sha256:` z plików-wskaźników
  `https://raw.githubusercontent.com/IgorSwat/Phonemis/<rev>/data/<język>/<plik>`.
- **Model**: nowa rewizja wymaga zmiany `plkokoro::REVISION`, `revision` w `nix/pkgs/kokoro-model.nix` i nowej kopii
  `nix/catalog.json` (asercja pilnuje zgodności `catalogVersion`).
- **nixpkgs flake**: `nix flake update` (devenv tego repo używa własnego `devenv.yaml`/`devenv.lock`).

Uwaga (fish): `set NAZWA wartość` bez `-gx` nie eksportuje zmiennej do procesów potomnych — użyj `set -gx`.

## Zweryfikowane i nie

Sprawdzone (2026-09-23, rustc 1.98.1, ORT 1.27.1):
- `devenv shell` tego repo + `pk-doctor` (same `OK`), `cargo test --workspace -- --test-threads=1` = 53 passed;
  synteza pl i de oraz krzyżowa (głos `df_anna`, fonemizer `pl`); nadpisania w `devenv.local.nix` i błędy opcji.
- **Projekt-konsument** z inputem `path:` do tego repo (`flake: false`) i `imports: - plkokoro/nix`,
  `plkokoro.voices = [ "pm_mateusz" "df_anna" ]`: zmienne ustawione, Phonemis `de pl`, model z oboma głosami
  (także wcześniej jako input flake z `inputs.nixpkgs.follows`).
- **Flake** (`nixos-unstable`, ORT 1.27.1): `packages.plkokoro` buduje się i działa z pustym środowiskiem (`env -i`);
  `lib.mkPlkokoro { voices = [ "pm_mateusz" "df_anna" ]; }` syntezuje po niemiecku.

**Nie sprawdzono**: inputu i flake przez `github:` (testowane lokalną ścieżką `path:`); systemów innych niż
x86_64-linux (flake deklaruje też aarch64-linux i darwin);
jakości fonemizacji i syntezy poza pl/de; języków Phonemis
`fr es it pt hi en-gb` (sumy z wskaźników LFS, bez budowania).
