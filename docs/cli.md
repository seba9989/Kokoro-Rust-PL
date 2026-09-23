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
| `--fetch-only` | wył. | pobierz model Kokoro (wybrany język i głos) do `--model-dir`, wypisz ścieżki i zakończ |
| `--list-voices` | wył. | wypisz głosy z `catalog.json` (język, id, nazwa, płeć, model, język fonemizera; `*` = domyślny) i zakończ |
| `--lang KOD` | `KOKORO_LANG`, potem `pl` | język mówcy (`pl`, `de`, `en-us`, `pt-br`…) — wyznacza głos, model i tokenizer |
| `--voice ID` | `KOKORO_VOICE`, potem domyślny głos języka | głos w języku mówcy (`--list-voices`) |
| `--phonemis-lang KOD` | `PHONEMIS_LANG`, potem frontend języka mówcy | język fonemizera (`pl`, `en-us`, `en-gb`, `de`, `fr`, `es`, `it`, `pt`, `hi`) |
| `--model-dir DIR` | `KOKORO_MODEL_DIR` | lokalna kopia modelu Kokoro |
| `--offline` | wył. | nigdy nie łącz się z HF (`KOKORO_OFFLINE=1` też) |
| `--providers A,B` | CPU | providery ORT po przecinku; wymaga budowy z odpowiednią cechą (`--features migraphx` itd.) |
| `--runner PLIK` | `PHONEMIS_RUNNER` | ścieżka do `phonemis_runner` |
| `--model PLIK` | `PHONEMIS_MODEL` lub wyprowadzone z runnera (`data/<język>/phonemizer_<język>.bin`) | wagi Phonemis dla języka fonemizera |
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

# inne języki i głosy (w devenv muszą być w plkokoro.voices / plkokoro.phonemisLanguages)
plkokoro --list-voices
plkokoro --lang de "Guten Tag. Über alles." -o de.wav                  # głos domyślny df_anna, fonemizer de
plkokoro --lang en-us --voice af_heart "Hello there." -o en.wav
plkokoro --lang en-us --voice af_heart --phonemis-lang pl "Dzień dobry." -o akcent.wav
plkokoro --lang ja --voice jf_alpha --ipa "koɲɲiʨiβa" -o ja.wav         # ja/zh: tylko gotowe IPA

# pobranie modelu raz, potem praca offline
plkokoro --fetch-only
plkokoro --offline "Działa bez sieci." -o offline.wav
```

## Typowe błędy

| Komunikat | Przyczyna i naprawa |
|---|---|
| `nie podano Phonemis: ustaw Config::phonemis_runner (albo PHONEMIS_RUNNER)…` | brak `--runner` i zmiennej; w devenv ustawia ją pakiet Nix |
| `… to wskaźnik Git LFS, a nie wagi modelu` | wagi nie pobrane: `git lfs pull --include='data/<język>/*'` w repo Phonemis |
| `brak wag Phonemis: …/data/<język>/phonemizer_<język>.bin` | brak wag języka fonemizera: w devenv dodaj język do `plkokoro.phonemisLanguages`; poza devenv podaj `--model` |
| `Phonemis nie obsługuje języka "…"` | `--phonemis-lang` spoza listy; dla ja/zh użyj `--ipa` |
| `nie znaleziono języka "…"` / `język "…" nie ma głosu "…"` | zły `--lang` / `--voice`; lista: `--list-voices` |
| `brak pliku modelu …, a katalog … jest tylko do odczytu` | głos spoza pakietu Nix: dodaj go do `plkokoro.voices` w `devenv.local.nix` albo wskaż `--model-dir` z prawem zapisu |
| `język mówcy "ja" nie ma frontendu tekstowego w Phonemis` | ja/zh: podaj IPA (`--ipa`) albo `--phonemis-lang` |
| `inicjalizacja ONNX Runtime (biblioteka "…")` | brak `libonnxruntime.so`: ustaw `--ort-lib` / `ORT_LIBRARY_PATH` (≥ 1.21) |
| `brak lokalnego pliku modelu: …` | tryb offline bez pobranego modelu: uruchom `--fetch-only` z siecią |
| `provider "…" nie jest dostępny: nieznany albo niewłączony w tej kompilacji` | zbuduj CLI z cechą providera (`--features …`); sam provider musi też być w załadowanej bibliotece ORT |
| `plkokoro: brak fonemów do syntezy …` | pusty tekst albo wszystkie znaki spoza słownika Kokoro |
| `phonemis_runner zakończył się kodem 1: … std::vector larger than max_size()` | uszkodzone wagi Phonemis |
| `speed musi być > 0` | `--speed` ≤ 0 |
