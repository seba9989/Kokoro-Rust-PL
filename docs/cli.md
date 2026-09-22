# Referencja CLI (`plkokoro-cli`, binarka `plkokoro`)

CLI to cienka warstwa nad czterema operacjami biblioteki: ładuje model, zamienia tekst na IPA, syntezuje, zapisuje
WAV i zwalnia model. Cała logika leży w bibliotece — w `main.rs` jest tylko parsowanie opcji (`clap`), czytanie
wejścia, logowanie, obsługa Ctrl+C i kody wyjścia.

```
cargo build --release -p plkokoro-cli        # -> target/release/plkokoro
plkokoro [opcje] [tekst...]
```

Flagi mogą stać **przed i po** tekście (`plkokoro "Cześć." -o out.wav`). Krótkie formy tylko: `-f`, `-o`, `-v`, `-h`;
reszta to `--długie`. (W wersji Go działało też `-speed`; tu nie.) Kilka słów pozycyjnych jest łączonych spacją.

## Opcje

| Opcja | Domyślnie | Znaczenie |
|---|---|---|
| `-f`, `--file PLIK` | — | wczytaj tekst z pliku UTF-8 (ma pierwszeństwo przed tekstem z linii poleceń) |
| `-o`, `--output PLIK` | `output_pl.wav` | plik wyjściowy WAV (PCM 16-bit, mono, 24 kHz) |
| `--speed X` | `1.0` | tempo mowy, musi być > 0 (**`0` to błąd**, inaczej niż w wersji Go) |
| `--max-phonemes N` | `300` | limit fonemów na porcję (twardo ≤ 510) |
| `--no-normalize` | wył. | wyłącz normalizację uzupełniającą (zł, %, skróty…) |
| `--ipa` | wył. | wejście to gotowe IPA — pomiń `text_to_ipa` |
| `--phonemize-only` | wył. | wypisz IPA na stdout i zakończ (bez ładowania Kokoro; włącza logi debug) |
| `--fetch-only` | wył. | pobierz model Kokoro do `--model-dir`, wypisz ścieżki i zakończ |
| `--model-dir DIR` | `KOKORO_MODEL_DIR` | lokalna kopia modelu Kokoro |
| `--offline` | wył. | nigdy nie łącz się z HF (`KOKORO_OFFLINE=1` też) |
| `--providers A,B` | CPU | providery ORT po przecinku; wymaga budowy z odpowiednią cechą (`--features migraphx` itd.) |
| `--runner PLIK` | `PHONEMIS_RUNNER` | ścieżka do `phonemis_runner` |
| `--model PLIK` | `PHONEMIS_MODEL` lub wyprowadzone z runnera | wagi Phonemis (`phonemizer_pl.bin`) |
| `--ort-lib PLIK` | `ORT_LIBRARY_PATH` | `libonnxruntime.so` (≥ 1.21) |
| `-v`, `--verbose` | wył. | pokaż normalizację i IPA każdego zdania |
| `-h`, `--help` | — | pomoc (kod 0) |

Brak tekstu i `--file` → tekst domyślny („Cześć, to jest test polskiej syntezy mowy Kokoro.”).

## Wyjście i logi

| Strumień | Zawartość |
|---|---|
| stdout | `Zapisano <plik> (<sekundy> s nagrania, wygenerowano w <sekundy> s)`; czas generowania liczy tylko `synthesize` (bez ładowania modelu i zapisu pliku); przy `--phonemize-only` samo IPA (zdanie na linię, pusta linia = akapit); przy `--fetch-only` ścieżki plików z rozmiarami |
| stderr | logi w formie `level=INFO msg=…` (INFO, z `-v` DEBUG: `[normalize]`, `[phonemis]`; ostrzeżenia `[uwaga]…`) i komunikaty błędów `błąd: …` |

Dzięki temu `plkokoro --phonemize-only "…" 2>/dev/null` daje czyste IPA do dalszej obróbki.

## Kody wyjścia

| Kod | Znaczenie |
|---|---|
| `0` | sukces albo `-h` |
| `1` | błąd wykonania: konfiguracja (brak runnera, ORT, modelu w trybie offline), fonemizacja, synteza, zapis pliku; także przerwanie |
| `2` | błąd składni opcji (nieznana flaga, zła wartość) — kod z `clap` |
| `130` | **drugi** Ctrl+C / SIGTERM: natychmiastowe wyjście (np. gdy zawiesza się pobieranie) |

**Ctrl+C / SIGTERM.** Pierwszy sygnał anuluje kooperacyjnie: podprocesy `phonemis_runner` są zabijane, a proces kończy się
kodem 1 z komunikatem `błąd: plkokoro: operacja przerwana (anulowano)` (test automatyczny
`sigint_cancels_running_phonemization`, poniżej 3 s). Drugi sygnał kończy proces od razu.

## Przykłady

```fish
# pełna synteza
plkokoro "Cześć, to jest test." -o out.wav

# tylko IPA, w tym ręczne fonemy i normalizacja
plkokoro --phonemize-only "Mam 123 zł, 5% rabatu. Lubię [Kokoro](/kɔkˈɔrɔ/)." 2>/dev/null

# synteza z gotowego IPA (np. poprawionego ręcznie)
plkokoro --phonemize-only -f tekst.txt 2>/dev/null > tekst.ipa
plkokoro --ipa -f tekst.ipa -o tekst.wav

# długi tekst z pliku, wolniej
plkokoro -f rozdzial.txt --speed 0.9 -o rozdzial.wav

# pobranie modelu raz, potem praca offline
plkokoro --fetch-only
plkokoro --offline "Działa bez sieci." -o offline.wav
```

## Typowe błędy

| Komunikat | Przyczyna i naprawa |
|---|---|
| `nie podano Phonemis: ustaw Config::phonemis_runner (albo PHONEMIS_RUNNER)…` | brak `--runner` i zmiennej; w devenv: `pk-phonemis-build` |
| `… to wskaźnik Git LFS, a nie wagi modelu` | wagi nie pobrane: `pk-phonemis-build` albo `git lfs pull --include='data/pl/*'` |
| `brak wag Phonemis: …/data/pl/phonemizer_pl.bin` | runner zainstalowany bez wag (`--no-lfs`) albo inny układ katalogów; podaj `--model` |
| `inicjalizacja ONNX Runtime (biblioteka "…")` | brak `libonnxruntime.so`: ustaw `--ort-lib` / `ORT_LIBRARY_PATH` (≥ 1.21) |
| `brak lokalnego pliku modelu: …` | tryb offline bez pobranego modelu: uruchom `--fetch-only` z siecią |
| `provider "…" nie jest dostępny: nieznany albo niewłączony w tej kompilacji` | zbuduj CLI z cechą providera (`--features …`); sam provider musi też być w załadowanej bibliotece ORT |
| `plkokoro: brak fonemów do syntezy …` | pusty tekst albo wszystkie znaki spoza słownika Kokoro |
| `phonemis_runner zakończył się kodem 1: … std::vector larger than max_size()` | uszkodzone wagi Phonemis |
| `speed musi być > 0` | `--speed` ≤ 0 |
