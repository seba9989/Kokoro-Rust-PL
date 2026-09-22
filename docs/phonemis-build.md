# Budowanie Phonemis (`phonemis-build.sh`)

`scripts/phonemis-build.sh` buduje `phonemis_runner` (upstream: <https://github.com/IgorSwat/Phonemis>, CMake, C++20) i
instaluje w folderze projektu **tylko to, co potrzebne w czasie działania**. W devenv wywołujesz go jako `pk-phonemis-build`.
Działa też bez devenv; wymaga: `git`, `cmake`, kompilatora C++20 i — dla wag — `git-lfs`.

## Tryb domyślny

Klon i build w **katalogu tymczasowym**, potem instalacja do `<projekt>/phonemis`, sprzątanie i test dymny:

```
phonemis/
├── build/phonemis_runner          runner (Release)                                  ~1 MB
├── data/pl/phonemizer_pl.bin      wagi PL z Git LFS                                 ~7 MB
├── LICENSE                        licencja MIT upstreamu (wymóg przy dystrybucji binarki)
└── BUILDINFO                      źródło, wersja, commit, typ, kompilator, data
```

Biblioteka (Rust, Go i Python) sama znajduje wagi na podstawie położenia runnera:
`<prefix>/build/phonemis_runner` → `<prefix>/data/pl/phonemizer_pl.bin`.

Kolejność kroków: klon (`--depth 1`, bez pobierania wag wszystkich języków) → wagi PL z LFS → konfiguracja i budowanie →
**instalacja** → sprzątanie katalogu tymczasowego → **test dymny na zainstalowanych plikach**. Test dymny uruchamia
zainstalowany runner na „Mam 123 zł.” i wymaga niepustego IPA; dzięki temu, że katalog tymczasowy już nie istnieje,
sprawdza wyłącznie to, co zostało na stałe.

Gwarancje (sprawdzane przez `scripts/test-phonemis-build.sh`):

- nieudane budowanie **nie rusza** wcześniej zainstalowanej wersji (pliki kopiowane przez `<plik>.new` + `mv`),
- katalog tymczasowy jest usuwany także po błędzie,
- zainstalowany runner nie ma RPATH-u ani zależności od katalogu tymczasowego i działa z dowolnego katalogu.

## Opcje

| Opcja | Znaczenie |
|---|---|
| `--prefix DIR` | gdzie zainstalować (domyślnie `<projekt>/phonemis`, czyli `<skrypt>/../phonemis`) |
| `--no-install` | bez instalacji; wynik zostaje w katalogu roboczym (wymaga `--dir`) |
| `--dir DIR` | **trwały** katalog roboczy zamiast tymczasowego: kolejne uruchomienia są przyrostowe (bez rekompilacji) |
| `--keep-tmp` | zostaw katalog tymczasowy (wypisze ścieżkę) |
| `--tmp-base DIR` | gdzie utworzyć katalog tymczasowy (domyślnie `$TMPDIR`, `/tmp`) |
| `--ref REF` | tag lub gałąź (np. `v0.1.0`) |
| `--update` | `git pull --ff-only` w katalogu roboczym (tylko z `--dir`, tylko na gałęzi; nie łącz z `--ref`) |
| `--clean` | usuń `build/` przed konfiguracją (tylko z `--dir`) |
| `-j`, `--jobs N` | równoległość (domyślnie `nproc`) |
| `--type TYP` | `Release` (domyślnie), `RelWithDebInfo`, `MinSizeRel`, `Debug` |
| `--tests` | zbuduj też `phonemis_test` (tylko z `--dir` albo `--keep-tmp`) |
| `--no-lfs` | nie pobieraj wag (runner zostanie zainstalowany bez wag) |
| `--no-verify` | pomiń test dymny |
| `--no-force-includes` | nie dodawaj wymuszonych `#include` |
| `--cmake-arg ARG` | dodatkowy argument dla cmake (powtarzalny), np. `-DET_ON=ON` |

Zmienne: `PHONEMIS_REPO_URL` (domyślnie repo upstreamu), `CXX`, `CXXFLAGS` (doklejane po wymuszonych nagłówkach), `TMPDIR`.
Niedozwolone kombinacje kończą się czytelnym błędem (np. `--update` bez `--dir`, bo katalog tymczasowy jest za każdym razem nowy).

## Przykłady

```fish
pk-phonemis-build                                 # tymczasowo -> ./phonemis; Release, wagi, test dymny
pk-phonemis-build --prefix ~/phonemis             # inny folder docelowy
pk-phonemis-build --ref v0.1.0                    # konkretna wersja
pk-phonemis-build --dir ~/cache/phonemis-src      # trwały katalog roboczy (szybkie kolejne budowania)
pk-phonemis-build --dir ~/cache/phonemis-src --update
pk-phonemis-build --dir ~/cache/phonemis-src --clean --type Debug --no-install
pk-phonemis-build --no-lfs --no-verify            # sama kompilacja runnera
```

## Dlaczego skrypt robi to, co robi

Trzy problemy upstreamu, każdy zaobserwowany i obsłużony:

1. **Brak domyślnego `CMAKE_BUILD_TYPE`.** Budowanie „jak w README upstreamu” (`cmake .. && make`) daje flagi
   `-std=gnu++20 -mavx2 -mfma` — **bez optymalizacji**. Skrypt używa `Release` (`-O3 -DNDEBUG`). Sprawdzenie
   istniejącego builda: `grep CMAKE_BUILD_TYPE <repo>/build/CMakeCache.txt` (pusta wartość = bez optymalizacji).
2. **Brakujące nagłówki standardowe** (`<optional>` w `num2word/layer.h`, `<string>` w `tokenizer/types.h`…). Nowsze
   libstdc++ nie dociągają ich przechodnio i build pada (`'optional' in namespace 'std' does not name a template type`).
   Skrypt wymusza nagłówki przez `-include`; `--no-force-includes` to wyłącza (test `S2` używa zepsutego upstreamu, by nie
   zależeć od kompilatora).
3. **Zmiana kompilatora w istniejącym `build/`.** CMake wyrzuca wtedy cache i konfiguruje od nowa, *gubiąc opcje z linii
   poleceń* (`-DBUILD_RUNNER=ON`) — skutek: `No rule to make target 'phonemis_runner'`. W Nixie ścieżka kompilatora zmienia się po
   każdej aktualizacji nixpkgs. Skrypt wykrywa zmianę, czyści `build/` i po konfiguracji sprawdza, że `BUILD_RUNNER=ON`.

Runner bez wag kończy się kodem 0 i zwraca same spacje — dlatego test dymny jest osobnym krokiem, a wskaźnik Git LFS
(~130 B zamiast ~7 MB) jest wykrywany po nagłówku `version https://git-lfs`, nie po rozmiarze.

## Ograniczenia i uwagi

- Runner jest zlinkowany **dynamicznie** (libstdc++, libc). W Nixie to ścieżki w `/nix/store`; po `nix-collect-garbage`
  może przestać się uruchamiać (`pk-doctor` to wykrywa; naprawa: ponownie `pk-phonemis-build`).
- Binarka jest skompilowana z `-mavx2 -mfma` (upstream włącza to na x86_64); kopiuj ją tylko na procesor z AVX2.
- `-DET_ON=ON` (ExecuTorch) wymaga własnego ExecuTorch i **nie był testowany**.
- Pomyślny test dymny na prawdziwych wagach z Git LFS nie był wykonany w środowisku, w którym powstał skrypt (zablokowany
  host LFS); wszystko poza nim jest testowane automatycznie — patrz [testy.md](testy.md), poziom 3 i 4.
- Wymaga GNU `stat`/`find`/`realpath`/`install` (Linux); pod macOS może wymagać ich GNU-odpowiedników.
