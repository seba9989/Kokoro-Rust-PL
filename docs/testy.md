# Pełna procedura testów

Testy są ułożone w poziomy od najtańszego i najbardziej odizolowanego do prawdziwych danych. Idź od góry: porażka
na niższym poziomie zwykle tłumaczy porażki wyżej. Poziomy 0–2 są automatyczne i nie potrzebują prawdziwych wag
ani modelu; poziom 4 to jedyne miejsce, gdzie sprawdza się, że **naprawdę działa** (prawdziwe wagi i model dostarczają
pakiety Nix z devenv).

| Poziom | Co | Polecenie | Wymaga | Dowodzi |
|---|---|---|---|---|
| 0 | środowisko | `pk-doctor` | devenv | zależności są na miejscu |
| 1 | logika, bez ORT | `env -u ORT_LIBRARY_PATH cargo test --workspace -- --nocapture` | Rust | tekst, zgodność z Pythonem, G2P, magazyn modelu, WAV, logika modelu na atrapie, CLI bez ORT |
| 2 | pełny zestaw z ORT | `pk-test` | ORT ≥ 1.21 | + inferencja na prawdziwej sesji ORT, cykl życia, współbieżność, CLI end-to-end |
| 3 | pakiety Nix | `nix-build` `nix/phonemis.nix`, `nix/kokoro-model.nix` | Nix, sieć | budowanie Phonemis, wagi i model z sumami SHA-256, test dymny runnera w każdym języku |
| 4 | akceptacja na prawdziwych danych | ręcznie, A1–A10 | devenv (pakiety Nix) | działa naprawdę; jakość mowy |
| 5 | zgodność z Pythonem | diff IPA + regeneracja korpusu | projekt Pythona | ten sam wynik co wersja referencyjna |

Wszystkie polecenia uruchamiaj w katalogu głównym projektu, w `devenv shell` (zmienne `ORT_LIBRARY_PATH`,
`PHONEMIS_RUNNER`, `KOKORO_MODEL_DIR` są już ustawione). Poza devenv ustaw je ręcznie (patrz [devenv.md](devenv.md)).

**Ważne — libtest nie ma prawdziwego „skip”.** Testy wymagające ORT bez `ORT_LIBRARY_PATH` wypisują `SKIP: …` i kończą
się od razu, więc `cargo test` pokazuje je jako *ok*. Zawsze sprawdzaj liczbę `SKIP` (poziom 1 i 2 poniżej).

---

## Poziom 0 — środowisko

```fish
pk-doctor
```

**Oczekiwane** (po zainstalowaniu Phonemis i pobraniu modelu): same linie `OK`, kod wyjścia 0.

```
plkokoro-rs — diagnostyka środowiska
  OK    rustc 1.xx… / cargo 1.xx…
  OK    libonnxruntime: /nix/store/…/lib/libonnxruntime.so
  OK    phonemis_runner: /nix/store/…-phonemis-…/build/phonemis_runner
  OK    wagi Phonemis (pl): /nix/store/…-phonemis-…/data/pl/phonemizer_pl.bin
  OK    model Kokoro: /nix/store/…-kokoro-model-v2.1.1
  ....  głos: pl/pm_mateusz
```

Po jednej linii `wagi Phonemis (…)` na każdy język z `plkokoro.phonemisLanguages` i `głos: …` na każdy głos z
`plkokoro.voices` ([devenv.md](devenv.md)).

---

## Poziom 1 — logika bez ONNX Runtime

```fish
env -u ORT_LIBRARY_PATH cargo test --workspace -- --nocapture 2>&1 | grep -E "^test result|SKIP:" | sort | uniq -c
```

**Kryterium zaliczenia:** kod wyjścia 0, brak `FAILED`. Sumaryczne liczby: **43 „passed”, z czego dokładnie 8 to SKIP**
(35 wykonanych): `lifecycle_real_ort`, `reload_and_two_models`, `options_validation_and_errors`,
`synth_options_pause_semantics`, `concurrent_use_and_unload`, `engine_rejects_model_with_wrong_io`,
`synthesize_wav_flags_after_text`, `ipa_and_file_input`. Każdy wypisuje powód `ustaw ORT_LIBRARY_PATH…`.

---

## Poziom 2 — pełny zestaw z ONNX Runtime

```fish
pk-test            # = cargo test --workspace
```

**Kryterium zaliczenia:** **53 passed, 0 failed, 0 SKIP**, kod wyjścia 0 (sprawdź: `pk-test -- --nocapture 2>&1 | grep -c SKIP:`
ma dać `0`). Zestaw po skompilowaniu trwa kilka sekund.

**Znany problem — uruchamiaj po kolei:** `pk-test -- --test-threads=1`. Równolegle `wav_engine` potrafi paść
SIGABRT-em (`OrtGetApiBase must be present`, 3 z 5 przebiegów): `ort 2.0.0-rc.13` po nieudanym `init_from` ze złą
ścieżką (test `wrong_ort_library_is_reported`) oznacza bibliotekę jako załadowaną, więc kolejna inicjalizacja w tym
samym procesie panikuje. Po kolei (kolejność alfabetyczna) wszystkie testy przechodzą. Bez ORT (`env -u
ORT_LIBRARY_PATH`) pomijanych jest 9 testów (`SKIP:`).

| Plik | Testy | Dowodzi |
|---|---|---|
| `plkokoro/src/lib.rs`, `text.rs` (moduły `tests`) | 15 testów jednostkowych | logika `Model` na atrapie silnika i backendu, bez ORT: cięcie na porcje i pauzy, znaki Unicode liczone w code pointach, wiersz stylu `n−1`, filtr słownika, walidacja opcji, `unload`/`Drop`, `Send + Sync`, anulowanie |
| doctest w `lib.rs` | 1 | przykład z dokumentacji kompiluje się (`no_run`) |
| `tests/parity.rs` | `parity_normalize` `…split_sentences` `…pack_ipa` `…overrides` | wynik Rust = wynik Pythona: 3538 + 3513 + 2500 + 6014 = **15 565** przypadków z `testdata/parity.json` (te same co w Go) |
| `tests/g2p.rs` | `runner_basics` `…config_errors` `…parallel_error_and_cancel` `…language_weights_and_english_extras` | parsowanie wyjścia runnera (ANSI), wyprowadzanie wag z położenia runnera, błędy (brak runnera, brak `+x`, wskaźnik LFS, brak wag), równoległość (8 zdań po 0,3 s w < 1,5 s), przerwanie podprocesów |
| `tests/store.rs` | `local_first_offline_and_fetch` `empty_file_is_redownloaded` `http_errors_and_truncation` `rejects_path_traversal_from_catalog` `catalog_without_polish_language` `download_selected_voice_and_list_voices` `read_only_model_dir_gives_actionable_error` `cancelled_download_leaves_nothing` | pobieranie tylko gdy brak pliku, tryb offline, pusty plik, HTTP 404, ucięta odpowiedź (bez śladu na dysku), odrzucanie `..` i ścieżek bezwzględnych, anulowanie |
| `tests/wav_engine.rs` | `encode_wav_header_and_samples` `engine_rejects_model_with_wrong_io` `unsupported_provider_is_rejected` `wrong_ort_library_is_reported` | nagłówek i próbki WAV (obcinanie), odrzucenie modelu o złym interfejsie, nieznany provider, zła biblioteka ORT |
| `tests/model.rs` | `lifecycle_real_ort` | pełny cykl na **prawdziwej sesji ORT**: liczba żądań HTTP, format IPA, **wartości próbek** (dowodzą, że `input_ids` z BOS/EOS, wiersz stylu `n−1` i `speed` trafiają do modelu), pauzy, wstawki ręczne, filtr słownika (ostrzeżenie raz), `unload` |
| | `reload_and_two_models` | dwa modele naraz, zwolnienie jednego nie psuje drugiego, ponowne ładowanie |
| | `options_validation_and_errors` | `speed ≤ 0`/`NaN`, `NoPhonemes`, błąd runnera propagowany, limit `max_phonemes`, anulowanie |
| | `synth_options_pause_semantics` | różnica z pauzami i bez = dokładnie jedna pauza zdaniowa; `..Default::default()` zachowuje pauzy |
| | `skip_kokoro_uses_no_network` `custom_g2p_and_cache` | `skip_kokoro` nie używa sieci, własny backend, cache, zamknięcie backendu w `unload` |
| | `concurrent_use_and_unload` | 8 wątków + `unload` w trakcie: tylko sukces albo `Unloaded` |
| | `phonemizer_language_follows_speaker_and_can_be_overridden` `language_without_frontend_and_bad_selection` | język fonemizera z katalogu (`de`, `pt-br` → `pt`) i nadpisanie, brak normalizacji PL dla `de`, `ja` bez G2P, błędy wyboru języka/głosu/wag |
| | `selected_voice_is_used_for_synthesis` | wybrany głos pobierany zamiast domyślnego, synteza na ORT; `ja` syntezuje z IPA bez runnera |
| `plkokoro-cli/tests/cli.rs` | `fetch_then_offline` | `--fetch-only`, potem `--offline` bez serwera |
| | `synthesize_wav_flags_after_text` | synteza end-to-end binarką, flagi po tekście, WAV 24 kHz mono 16-bit, drugie uruchomienie po wyłączeniu serwera |
| | `phonemize_only_no_kokoro` `ipa_and_file_input` | `--phonemize-only` (czyste IPA, zero żądań), `--no-normalize`, `PHONEMIS_RUNNER` z env, `--ipa`, `-f` |
| | `errors_and_exit_codes` | kody wyjścia 0/1/2 i komunikaty (brak runnera, zły runner, zły provider, offline bez modelu, `speed 0`, `KOKORO_OFFLINE`, `-h`…) |
| | `list_voices_and_language_selection` | `--list-voices`, `--lang`/`--voice`/`--phonemis-lang` i `KOKORO_LANG` trafiają do runnera, błąd złego głosu |
| | `sigint_cancels_running_phonemization` | **SIGINT w trakcie fonemizacji**: kod 1, „anulowano”, poniżej 3 s (w wersji Go tylko ręcznie) |

Atrapa modelu (`testdata/tiny_kokoro.onnx`) ma interfejs Kokoro, a wyjście zależy od wszystkich wejść — dlatego testy
z prawdziwym ORT wykrywają błędne przekazanie któregokolwiek z nich. **Nie sprawdza to jakości mowy** (poziom 4).

### Uruchamianie wybranych testów

```fish
cargo test -p plkokoro --test parity -- --nocapture          # tylko zgodność z Pythonem (wypisuje liczniki)
cargo test -p plkokoro --test model lifecycle_real_ort       # cykl życia na prawdziwym ORT
cargo test -p plkokoro-cli                                   # tylko CLI
cargo test --release -p plkokoro-cli                         # CLI na binarce release
for i in (seq 20); cargo test -p plkokoro --test model concurrent -q; end   # powtórzenia (rzadkie wyścigi)
```

### Rozszerzona zgodność (przed wydaniem)

Wersje ORT (biblioteka ≥ 1.21):

```fish
for v in 1.21.0 1.22.0 1.23.2 1.30.0
    curl -sL -o /tmp/ort-$v.tgz https://github.com/microsoft/onnxruntime/releases/download/v$v/onnxruntime-linux-x64-$v.tgz
    tar -xzf /tmp/ort-$v.tgz -C /tmp
    echo "== ORT $v"; env ORT_LIBRARY_PATH=/tmp/onnxruntime-linux-x64-$v/lib/libonnxruntime.so cargo test --workspace
end
```

**Sprawdź też kod wyjścia całego procesu** (nie tylko „test result: ok”): ORT 1.21/1.22 potrafią kończyć proces SIGSEGV-em
po poprawnym przebiegu (obejście: [dzialanie.md](dzialanie.md) §6). `cargo` zgłasza to jako `process didn't exit
successfully … SIGSEGV` mimo zielonych testów.

Cechy providerów (kompilacja) i głośny błąd braku providera:

```fish
for f in cuda tensorrt rocm migraphx coreml directml openvino; cargo check -p plkokoro --features $f; end
cargo test -p plkokoro --features migraphx --test wav_engine unavailable_provider_fails_loudly -- --nocapture
```

Jakość kodu: `cargo fmt --all -- --check` (konfiguracja w `rustfmt.toml`) i `cargo clippy --workspace --all-targets` —
oba czyste na rustc 1.91.1.

**Dotąd zaliczone:** ORT 1.21.0, 1.22.0, 1.23.2, 1.30.0 (Linux x86_64), rustc 1.91.1: `cargo test --workspace` = 43 passed,
kod wyjścia 0, na każdej wersji; testy CLI także na binarce release (ORT 1.22.0 i 1.21.0). Po dodaniu wyboru języka
i głosu (2026-09-23): ORT 1.27.1 z nixpkgs, rustc 1.98.1, `--test-threads=1` = 53 passed.

---

## Poziom 3 — pakiety Nix

```fish
nix-build --no-out-link -E 'with import <nixpkgs> {}; callPackage ./nix/phonemis.nix { languages = [ "pl" "de" "en-us" ]; }'
nix-build --no-out-link -E 'with import <nixpkgs> {}; callPackage ./nix/kokoro-model.nix { voices = [ "pm_mateusz" "df_anna" ]; }'
```

**Kryterium zaliczenia:** oba kończą się ścieżką w `/nix/store`. `nix/phonemis.nix` ma `installCheckPhase`: runner
fonemizuje „Test 123.” w każdym języku i musi dać niepuste IPA (w logu `phonemis pl: tˈɛst stˈɔ dvadʒˈɛɕtɕa tʃˈɨ.`).
Złe sumy SHA-256 przerywają pobieranie. Nieznany głos/język przerywa już ewaluację z listą dostępnych.

---

## Poziom 4 — akceptacja na prawdziwych danych (ręcznie)

Jedyny poziom, który sprawdza to, czego atrapy nie potrafią: prawdziwe wagi Phonemis, prawdziwy model Kokoro,
zgodność fonemów Phonemis ze słownikiem Kokoro i jakość mowy. A1–A5 i A10 wykonane 2026-09-23 (pakiety Nix,
ORT 1.27.1). Wykonaj po kolei; przy porażce zatrzymaj się.

**A1.** `devenv shell` — przy pierwszym wejściu Nix buduje Phonemis i pobiera model (kilkaset MB); kolejne wejścia są
natychmiastowe.

**A2.** `pk-doctor` — żadnej linii `BRAK`.

**A3. Sam frontend.**
```fish
cargo run -q --release -p plkokoro-cli -- --phonemize-only "Mam 123 zł, 5% rabatu." 2>/dev/null
```
Oczekiwane (identyczne z wersją Pythona i Go): `mˈam stˈɔ dvadʒˈɛɕtɕa tʃˈɨ zwˈɔtɛ, pʲˈɛɲtɕ prˈɔʦɛnt rabˈatu.`
Ręczne fonemy: `… --phonemize-only "Lubię [Kokoro](/kɔkˈɔrɔ/)."` — `kɔkˈɔrɔ` ma stać dosłownie.

**A4. Model:** `cargo run -q -p plkokoro-cli -- --fetch-only --offline` — 4 linie ze ścieżkami w `/nix/store` i rozmiarami.

**A5. Pełna synteza — najważniejszy test.**
```fish
cargo run -q --release -p plkokoro-cli -- "Cześć, to jest test polskiej syntezy mowy Kokoro." -o out.wav -v 2>&1 | grep -E "uwaga|Zapisano|błąd"
file out.wav
```
Oczekiwane: `Zapisano out.wav (… s nagrania, wygenerowano w … s)` i `WAVE audio, Microsoft PCM, 16 bit, mono 24000 Hz`. **Odsłuchaj plik.** Komunikat
`[uwaga] znaki spoza słownika Kokoro zostaną pominięte` oznacza fonemy Phonemis spoza słownika Kokoro — zanotuj znaki.

**A6. Akapity, pauzy, normalizacja.**
```fish
printf 'Pierwszy akapit. Drugie zdanie.\n\nDrugi akapit: 21 osób, 5 zł, godz. 12:30, ok. 5 minut.\n' > /tmp/t.txt
cargo run -q --release -p plkokoro-cli -- -f /tmp/t.txt --speed 0.9 -o akapity.wav
```
Na nagraniu: krótka pauza między zdaniami, dłuższa między akapitami; liczby, „5 złotych”, „dwanaście trzydzieści”,
„około pięć minut” wymówione słownie.

**A7. Wydajność.** `pk-build`, potem `time ./bin/plkokoro "…" -o /tmp/p.wav`. Zanotuj czas i długość nagrania.
Każde zdanie startuje osobny proces runnera i ładuje wagi — przy długim tekście zmierz osobno `--phonemize-only`.

**A8. Pamięć po `unload`.**
```fish
cargo run --release -p plkokoro --example memory
```
Z atrapą modelu RSS po `unload` spadał tylko o ok. 1 MB (27,0 → 26,0 MB na ORT 1.22.0; 33,5 → 32,5 MB na 1.30.0),
bo biblioteka ORT zostaje zmapowana. Z prawdziwym Kokoro oczekuj spadku rzędu rozmiaru wag modelu — **to jest do
zmierzenia**. Jeśli spadek jest znikomy także tam, rozważ oddzielny proces dla syntezy.

**A9. Przerwanie.** Uruchom syntezę długiego tekstu i naciśnij Ctrl+C: oczekiwane `błąd: plkokoro: operacja przerwana
(anulowano)`, kod 1, brak procesów `phonemis_runner` w `ps`. (Automatycznie sprawdza to `sigint_cancels_running_phonemization`
z fałszywym runnerem; tu z prawdziwym.)

**A10. Inne języki i głosy.** W `devenv.local.nix`: `plkokoro.voices = [ "pm_mateusz" "df_anna" ];`, potem:
```fish
plkokoro --lang de --phonemize-only "Es kostet 5 Euro. Über alles." 2>/dev/null   # ɛs kˈɔstət fˈynf ˈɔøroː. / ˈyːbɜ ˈaləs.
plkokoro --lang de "Guten Tag. Über alles." -o de.wav
plkokoro --lang de --phonemis-lang pl "Dzień dobry, mam 5 zł." -o akcent.wav
plkokoro --lang en-us "Hello."      # błąd: brak wag en-us / głosu spoza pakietu — z podpowiedzią
```
**Odsłuchaj** `de.wav` i `akcent.wav` (polski tekst niemieckim głosem).

---

## Poziom 5 — zgodność z Pythonem

```fish
set T "Mam 123 zł, 5% rabatu. Lubię [Kokoro](/kɔkˈɔrɔ/). Start o godz. 12:30, np. w piątek. „Cześć” – powiedziała."
cargo run -q --release -p plkokoro-cli -- --phonemize-only $T 2>/dev/null > /tmp/ipa_rs.txt
# w projekcie Pythona (~/Dokumenty/AI/Kokoro), tym samym T:
uv run ./tests/pl_kokoro_cli.py --phonemize-only $T 2>/dev/null > /tmp/ipa_py.txt
diff /tmp/ipa_rs.txt /tmp/ipa_py.txt && echo "IPA identyczne"
```

Korpus `testdata/parity.json` regeneruj, gdy zmienia się normalizacja lub dzielenie w Pythonie:

```fish
python3 testdata/gen_parity.py ~/Dokumenty/AI/Kokoro/tests > testdata/parity.json
cargo test -p plkokoro --test parity -- --nocapture
```

Generator ma stałe ziarno (`20260921`). Cyfry nie-ASCII są świadomie poza korpusem.

---

## Dane testowe i ich regeneracja

| Plik | Po co | Regeneracja |
|---|---|---|
| `testdata/parity.json` | referencyjne wyniki Pythona | `gen_parity.py` (wyżej) |
| `testdata/tiny_kokoro.onnx` | atrapa modelu (interfejs Kokoro) | `python3 testdata/gen_tiny_model.py testdata/tiny_kokoro.onnx` (pakiet `onnx`) |
| `testdata/bad_io.onnx` | model o złych nazwach wejść | `python3 testdata/gen_tiny_model.py testdata/bad_io.onnx bad` |

Fałszywy serwer HF, fałszywy runner i fałszywe wagi powstają w `plkokoro/tests/common/mod.rs`; testy CLI włączają ten
sam plik przez `#[path]`. Zachowanie fałszywego runnera zależy od **treści tekstu** (`BOOM` = błąd, `SLOW` = 0,3 s,
`HANG` = wisi 30 s), nie od zmiennych środowiskowych — testy idą równolegle w jednym procesie.

---

## Czego testy NIE pokrywają

- **Jakość mowy i prawdziwy Kokoro** — tylko poziom 4 (A5–A6), odsłuchem.
- **Zgodność fonemów Phonemis ze słownikiem Kokoro** — widoczna dopiero w A5 jako ostrzeżenie `[uwaga]`.
- **Jakość fonemizacji i mowy poza pl/de** — wagi `fr es it pt hi en-gb` mają sumy z wskaźników LFS, ale nie były
  budowane; `en-us` zbudowane i fonemizuje, synteza angielska nie była odsłuchana.
- **Prawdziwy Hugging Face** (CDN/xet, tokeny, limity, HTTPS/TLS) — serwer testowy udaje HF po zwykłym HTTP, w tym
  przekierowanie; **TLS (`rustls`) nie był ćwiczony**.
- **Providery ORT inne niż CPU na sprzęcie** — kompilują się i dają błąd przy braku providera (sprawdzone MIGraphX),
  ale nie były uruchamiane na GPU (CUDA, ROCm, MIGraphX, …).
- **Zwolnienie pamięci z prawdziwym modelem** (A8) — z atrapą sama biblioteka ORT dominuje pomiar.
- **Systemy i architektury poza Linux x86_64**.
- **Nadpisanie `plkokoro.voices` przez prawdziwy `devenv.local.nix`** — pakiety z innymi listami budowane `nix-build`.
- **Drugi Ctrl+C (kod 130)** i zawieszone połączenie w trakcie odczytu ciała pobieranego pliku.
- **Testy upstreamu Phonemis** (`phonemis_test`) — nie uruchamiane.

---

## Diagnostyka porażek

| Porażka | Prawdopodobna przyczyna |
|---|---|
| wiele `SKIP` z „ustaw ORT_LIBRARY_PATH” | brak zmiennej — na poziomie 2 to błąd konfiguracji, nie zaliczenie |
| `process didn't exit successfully … SIGSEGV` przy zielonych testach | ORT 1.21/1.22 przy zamykaniu procesu; obejście w `engine.rs` (`ensure_ort`) — sprawdź, czy nie zostało usunięte |
| asercja C++ `OrtEpDevice … front() … empty` przy tworzeniu sesji | cecha `api-22` (lub nowsza) w `ort` na ORT 1.22.0 — patrz [dzialanie.md](dzialanie.md) §7; zostaw `api-21` |
| `inicjalizacja ONNX Runtime … BadVersion` | `libonnxruntime.so` starsze niż 1.21 albo zła ścieżka |
| `lifecycle_real_ort`: zła wartość próbki | zmieniono przekazywanie `input_ids`/stylu/`speed` albo atrapę modelu |
| `parity_*` | zmieniono normalizację/dzielenie po jednej stronie; zregeneruj korpus (poziom 5) i rozstrzygnij, która implementacja jest poprawna |
| `runner_parallel_error_and_cancel`: zbyt długo | wolna maszyna (próg 1,5 s dla 8 zdań po 0,3 s) albo jeden rdzeń (`workers`) |
| `http_errors_and_truncation` wisi | brak limitów czasu w kliencie HTTP albo serwer testowy nie zamyka połączenia (używamy surowego TCP, nie `tiny_http`) |
| `SIGABRT` w `wav_engine` przy równoległym `pk-test` | znany problem `ort` (poziom 2) — uruchom `pk-test -- --test-threads=1` |
| budowanie `nix/phonemis.nix` pada na kompilacji | nowszy kompilator wymaga kolejnych nagłówków w liście `-include` (`NIX_CFLAGS_COMPILE`) |
| `hash mismatch` w `nix-build` | zmieniony plik po stronie HF/GitHub — sprawdź rewizję i sumy ([devenv.md](devenv.md), „Podbijanie wersji”) |
| A5: cisza lub szum | zły model/głos w katalogu modelu (np. `KOKORO_MODEL_DIR` nadpisany ręcznie) |

---

## Checklista przed wydaniem

```fish
cargo fmt --all -- --check                            # ma nic nie wypisać
cargo clippy --workspace --all-targets                # bez ostrzeżeń
pk-test -- --test-threads=1                           # poziom 2: 53 passed, 0 SKIP
# rozszerzona zgodność (ORT 1.21.0 / 1.22.0 / 1.23.2 / 1.30.0, sprawdź kody wyjścia) — patrz poziom 2
# pakiety Nix — poziom 3
# akceptacja A1–A10 na prawdziwych danych — poziom 4
git status --short                                    # brak niezamierzonych plików (target/, phonemis/, models/, *.wav są w .gitignore)
```
