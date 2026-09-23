# Środowisko devenv (Nix)

Pliki: `devenv.nix`, `devenv.yaml`, `nix/phonemis.nix`, `nix/kokoro-model.nix` (moduły devenv, wczytywane przez
`imports` w `devenv.nix`), `nix/catalog.json`.

Wszystkie zależności czasu działania są **deklaratywnymi pakietami Nix** (ten sam wzór co w aplikacji Koko-Anime):
przypięta rewizja, sumy SHA-256, budowane i pobierane przez `devenv shell` hermetycznie, cache'owane jak każdy pakiet.
Nic nie jest klonowane, kompilowane ani pobierane w `enterShell`.

| Pakiet (moduł) | Źródło | Zawartość |
|---|---|---|
| `pkgs.onnxruntime` (`devenv.nix`) | nixpkgs | `libonnxruntime.so` (ładowana dynamicznie przez crate `ort`) |
| `nix/phonemis.nix` — opcje `plkokoro.phonemisLanguages`, `plkokoro.phonemis`; ustawia `PHONEMIS_RUNNER` | `IgorSwat/Phonemis` @ `71eb1ce` (CMake) + wagi z Git LFS (`fetchurl`, sha256 = `oid` ze wskaźnika LFS) | `build/phonemis_runner`, `data/<język>/phonemizer_<język>.bin` (+ `lexicon_full.json`, `tagger.json` dla `en-us`/`en-gb`) dla języków z `plkokoro.phonemisLanguages`; `installCheckPhase` fonemizuje „Test 123.” w każdym języku |
| `nix/kokoro-model.nix` — opcje `plkokoro.voices`, `plkokoro.kokoroModel`; ustawia `KOKORO_MODEL_DIR` | `Shusek00/kokoro-kmp-models` @ `v2.1.1` (`fetchurl`, sumy z `nix/catalog.json`) | `v2.1.1/catalog.json` (prawdziwy, pełny), głosy z `plkokoro.voices`, ich modele ONNX i tokenizery — układ `plkokoro::Store` |

`nix/catalog.json` to kopia katalogu z Hugging Face (sprawdzona `sha256sum`); czytana przy ewaluacji, więc ścieżki
i sumy artefaktów pochodzą z niej bez import-from-derivation.

## Opcje (`devenv.local.nix`)

devenv automatycznie wczytuje `devenv.local.nix` (jest w `.gitignore`):

```nix
{ lib, ... }: {
  # głosy spakowane do KOKORO_MODEL_DIR (id z --list-voices); każdy nowy model to ~325 MB
  plkokoro.voices = [ "pm_mateusz" "df_anna" "af_heart" ];
  # opcjonalnie: języki Phonemis (domyślnie: frontendy języków wybranych głosów + pl)
  plkokoro.phonemisLanguages = [ "pl" "de" "en-us" "fr" ];
}
```

| Opcja | Domyślnie | Znaczenie |
|---|---|---|
| `plkokoro.voices` | `[ "pm_mateusz" ]` | głosy Kokoro w pakiecie modelu; nieznany głos przerywa ewaluację z podpowiedzią |
| `plkokoro.phonemisLanguages` | `textFrontend.language` języków wybranych głosów (z katalogu) + `pl` | wagi Phonemis instalowane obok runnera; typ `enum`: `pl en-us en-gb de fr es it pt hi` |
| `plkokoro.kokoroModel`, `plkokoro.phonemis` | — (tylko do odczytu) | zbudowane pakiety, np. do użycia w innych modułach (`config.plkokoro.phonemis`) |

Katalog modelu leży w `/nix/store` (tylko do odczytu): głos spoza `plkokoro.voices` daje błąd „katalog … jest tylko do
odczytu — dodaj głos do plkokoro.voices”. Żeby biblioteka pobierała dowolne głosy sama, wskaż katalog z prawem zapisu:
`env.KOKORO_MODEL_DIR = "/home/…/models/kokoro-kmp-models";`.

`KOKORO_LANG` / `KOKORO_VOICE` celowo **nie** są ustawiane przez devenv (zmieniałyby domyślne zachowanie testów);
język i głos wybieraj flagami CLI (`--lang`, `--voice`) albo w `Config`.

## Co ustawia `devenv.nix`

| Element | Wartość |
|---|---|
| Rust | `languages.rust.enable` (kanał `nixpkgs`: rustc, cargo, clippy, rustfmt, rust-analyzer). Nowszy toolchain: `languages.rust.channel = "stable"` (wymaga inputu `rust-overlay`) |
| kompilator C | `stdenv` devenv (zależność `ring` w `ureq`/`rustls` buduje C) |
| pakiety | `git`, pakiet Phonemis, pakiet modelu Kokoro |
| `ORT_LIBRARY_PATH` | `libonnxruntime.so` z nixpkgs |
| `PHONEMIS_RUNNER` | `<pakiet phonemis>/build/phonemis_runner` (niski priorytet: `lib.mkDefault`); wagi biblioteka wyprowadza z położenia runnera |
| `KOKORO_MODEL_DIR` | `<pakiet kokoro-model>` (niski priorytet) |
| asercje | ORT ≥ 1.21 i rustc ≥ 1.88 — inaczej wejście do powłoki przerywa się z komunikatem |

## Polecenia

Prefiks `pk-` nie przesłania poleceń powłoki (np. `test`).

| Polecenie | Robi |
|---|---|
| `pk-doctor` | sprawdza: `rustc`/`cargo`, `libonnxruntime`, runner (istnieje, uruchamia się), wagi każdego języka z `plkokoro.phonemisLanguages`, model Kokoro; wypisuje głosy obecne w `KOKORO_MODEL_DIR` |
| `pk-build` | `cargo build --release -p plkokoro-cli` i kopia binarki do `bin/plkokoro` |
| `pk-test` | `cargo test --workspace` (dodatkowe argumenty przekazywane, np. `pk-test lifecycle -- --nocapture`) |

Dawne `pk-phonemis-build`, `pk-fetch-model` i `pk-test-phonemis` (skrypt budujący w `/tmp`) zastąpiły pakiety Nix.
Katalogi `phonemis/` i `models/` w projekcie, jeśli zostały z tamtego układu, nie są już używane.

## Podbijanie wersji

- **Phonemis**: nowa rewizja w `nix/phonemis.nix` (`rev`), suma źródeł z
  `nix-prefetch-url --unpack https://github.com/IgorSwat/Phonemis/archive/<rev>.tar.gz`, a sumy wag — pola `oid sha256:`
  z plików-wskaźników `https://raw.githubusercontent.com/IgorSwat/Phonemis/<rev>/data/<język>/<plik>`.
- **Model**: nowa rewizja wymaga zmiany `plkokoro::REVISION`, `revision` w `nix/kokoro-model.nix` i nowej kopii
  `nix/catalog.json` (asercja pilnuje zgodności `catalogVersion`).

Pakiety buduje `devenv shell` (np. `devenv shell -- true` po zmianie listy w `devenv.local.nix`). Nieznany głos
przerywa ewaluację komunikatem `plkokoro.voices: nieznane głosy …`, a nieobsługiwany język — błędem typu opcji
`plkokoro.phonemisLanguages` z listą dozwolonych.

Uwaga (fish): `set NAZWA wartość` bez `-gx` nie eksportuje zmiennej do procesów potomnych — użyj `set -gx`.

## Zweryfikowane i nie

Sprawdzone (2026-09-23, nixpkgs rolling: rustc 1.98.1, ORT 1.27.1): `devenv shell` + `pk-doctor` (same `OK`);
`nix/phonemis.nix` buduje się dla `pl de en-us` i test dymny daje IPA w każdym języku; `nix/kokoro-model.nix` dla
`pm_mateusz df_anna` (oba modele, jeden tokenizer); synteza pl i de oraz krzyżowa (głos `df_anna`, fonemizer `pl`)
z plików pakietów; czytelny błąd dla głosu spoza pakietu; `cargo test --workspace` w tej powłoce.

Nadpisanie przez `devenv.local.nix` (`plkokoro.voices = [ "pm_mateusz" "df_anna" ]` → Phonemis `de pl`) i oba
błędy opcji sprawdzone po przeniesieniu opcji do modułów.

**Nie sprawdzono**: jakości fonemizacji i syntezy poza pl/de; języków Phonemis `fr es it pt hi en-gb` (sumy z
wskaźników LFS, bez budowania).
