# Jak to działa

## 1. Przegląd

```
        TEKST
          │
          ▼
 ┌─────────────────────┐   [tekst](/ipa/) wyciągane PRZED normalizacją i zastępowane znacznikiem
 │ 1. ręczne fonemy    │   (U+E000 · znak z bloku prywatnego · U+E001), by przeżyły dalsze etapy
 └─────────┬───────────┘
           ▼
 ┌─────────────────────┐   zł, %, &, skróty, godziny, cudzysłowy   (normalize_pl; można wyłączyć)
 │ 2. normalizacja PL  │   (tylko fonemizer pl)
 └─────────┬───────────┘
           ▼
 ┌─────────────────────┐   akapity (\n) → zdania (koniec zdania + wielka litera)
 │ 3. podział          │
 └─────────┬───────────┘
           ▼
 ┌─────────────────────┐   kawałki tekstu (bez wstawek ręcznych) → phonemis_runner, równolegle, z cache;
 │ 4. G2P (Phonemis)   │   Phonemis: Trim → Num2Word (liczby) → tokenizer → sieć Protophone → IPA
 └─────────┬───────────┘
           ▼
     IPA: jedno zdanie na linię, pusta linia = akapit          ◄── wynik text_to_ipa
           │
           ▼                                                     ◄── wejście synthesize
 ┌─────────────────────┐   znaki spoza słownika Kokoro odpadają (ostrzeżenie raz na znak)
 │ 5. filtr słownika   │
 └─────────┬───────────┘
           ▼
 ┌─────────────────────┐   każda linia → porcje ≤ max_phonemes (domyślnie 300, twardo ≤ 510)
 │ 6. cięcie na porcje │
 └─────────┬───────────┘
           ▼
 ┌─────────────────────┐   ids = [0] + tokeny + [0];  styl = wiersz (n−1) z macierzy 510×256;  speed
 │ 7. Kokoro (ONNX)    │   → waveform float32 (24 kHz, mono)
 └─────────┬───────────┘
           ▼
 ┌─────────────────────┐   cisza: między porcjami zdania / zdaniami / akapitami
 │ 8. sklejanie        │
 └─────────┬───────────┘
           ▼
     Vec<f32>  (SAMPLE_RATE = 24000 Hz)
```

Etapy 1–4 to `Model::text_to_ipa`, etapy 5–8 to `Model::synthesize`. Rozdzielenie jest celowe: IPA można obejrzeć, poprawić
ręcznie albo wygenerować innym narzędziem, a potem zsyntezować.

## 2. Format IPA

`text_to_ipa` zwraca, a `synthesize` przyjmuje napis, w którym:

- **jedno zdanie = jedna linia**,
- **pusta linia = koniec akapitu**,
- zwykły jednowierszowy napis IPA też jest poprawny (zostanie pocięty na porcje).

```
mˈam stˈɔ dvadʒˈɛɕtɕa tʃˈɨ zwˈɔtɛ, pʲˈɛɲtɕ prˈɔʦɛnt rabˈatu.      ← prawdziwy wynik dla „Mam 123 zł, 5% rabatu.”
(drugie zdanie tego samego akapitu)                                 ← ilustracja formatu
                                                                    ← pusta linia = nowy akapit
(pierwsze zdanie nowego akapitu)
```

Od tego zależą pauzy: `sentence_pause` po zdaniu, `paragraph_pause` po ostatnim zdaniu akapitu,
`clause_pause` między porcjami jednego zdania. Po ostatniej porcji całości pauzy nie ma.

## 3. Etapy tekstowe

### 3.1 Ręczne fonemy `[tekst](/ipa/)`

Składnia jak w Kokoro/misaki: `Lubię [Kokoro](/kɔkˈɔrɔ/).` Zawartość między ukośnikami trafia do modelu
dosłownie (po filtrze słownika), a Phonemis dostaje tylko resztę zdania. Tekst w `[...]` to tylko opis —
nie jest fonemizowany.

- Puste `//` = zwykły tekst (`[Kokoro](//)` jest fonemizowane normalnie).
- Nawiasy bez pary (`[inne]`, `(/x/)`) zostają bez zmian.
- Wstawka jest wyciągana przed normalizacją, więc `:`, cyfry, `%` czy `&` w IPA nie zostaną zmienione.
- Zdanie może zaczynać się od wstawki i nie skleja się z poprzednim.
- Limit: 6143 wstawki na wywołanie (blok znaków prywatnych Unicode); nadmiarowe są traktowane jak zwykły tekst.
- Uwaga: IPA w notacji angielskiej (`O`, `ɹ` z misaki) jest przepuszczane, ale polski głos może je wymówić dziwnie.

### 3.2 Normalizacja (`normalize_pl`)

Uzupełnia to, czego Phonemis PL nie robi. Działa **tylko dla fonemizera `pl`** (dla innych języków tekst trafia do
Phonemis bez tego etapu). Heurystyki, świadomie ograniczone:

| Wejście | Wynik | Uwagi |
|---|---|---|
| `123 zł`, `1 000 zł`, `PLN` | `123 złote`, `1000 złotych` | forma: 1 → *złoty*; kończy się na 2–4 (poza 12–14) → *złote*; reszta → *złotych*; ułamek → *złotych* |
| `5%` | `5 procent` | |
| `12:30`, `18:00` | `12 30`, `18` | minuty `00` znikają |
| `godz. 12:30` | `12 30` | `godz.` usuwane tylko przed cyfrą |
| `ok. 5 minut` | `około 5 minut` | tylko przed cyfrą; samo „ok.” zostaje |
| `np.`, `itd.`, `itp.`, `tzn.`, `tel.` | rozwinięcia | tylko rozwinięcia nieodmienne; `ul.`, `tzw.` celowo pominięte |
| `&` | ` i ` | |
| `„ ” “ « »` | `"` | `’ ‘` → `'`, `–` → `—`, spacja twarda → spacja |

Liczby na słowa zamienia sam Phonemis (`Num2Word`), tylko w mianowniku. Znane skutki: rok wychodzi
jako „dwa tysiące dwadzieścia cztery” w każdym przypadku; `27 marca 2026` nie jest czytane jako data
(`27.03.2026` — tak). Cyfry rozpoznawane w wyrażeniach są tylko ASCII (jak w wersji Go; Python łapie też inne cyfry Unicode).

### 3.3 Podział na zdania

Akapity dzieli nowa linia. Granica zdania to biały znak po `. ! ? …` (opcjonalnie z domykającym `" ” » )`),
po którym następuje **wielka litera** (opcjonalnie po `" „ “ ( «`) albo wstawka ręczna. Warunek wielkiej litery
zapobiega fałszywym cięciom po skrótach: w „Czwarte, np. kot.” nie ma granicy po „np.”. Dla fonemizera `pl` wielka
litera to ASCII albo `ĄĆĘŁŃÓŚŹŻ` (jak w Pythonie, pilnuje tego `parity.rs`); dla innych języków — każda wielka litera
Unicode („Über”, „École”).

### 3.4 G2P (Phonemis)

Kawałki tekstu (bez wstawek) idą do `phonemis_runner` — jeden podproces na kawałek, równolegle
(domyślnie `min(8, CPU)` procesów), wyniki w cache (do 20 000 wpisów, potem czyszczony). Runner ładuje
wagi przy każdym wywołaniu — stąd równoległość i cache. Limit czasu jednego wywołania: 60 s. Po anulowaniu
kontekstu podprocesy są zabijane (z 250 ms na domknięcie rur).

Runner bez `--model` kończy się kodem 0 i zwraca same spacje — dlatego biblioteka **zawsze** podaje wagi
i sprawdza je z góry (brak pliku i wskaźnik Git LFS dają czytelny błąd).

Język fonemizera (`--lang` runnera) to `Config::phonemis_lang`, domyślnie `textFrontend.language` języka mówcy
z `catalog.json` (np. `pt-br` → `pt`). Wagi: `<repo>/data/<język>/phonemizer_<język z _>.bin`; dla `en-us`/`en-gb`
leżące obok `lexicon_full.json` i `tagger.json` idą do runnera jako `--lexicon`/`--tagger`. Języki `ja` i `zh` nie mają
frontendu w Phonemis (`external-required` w katalogu) — model ładuje się wtedy bez G2P i przyjmuje tylko gotowe IPA.

## 4. Etapy syntezy

- **Filtr słownika (5).** Słownik Kokoro (`{znak: id}`) pochodzi z pliku tokenizera z `catalog.json`. Znaki spoza niego
  są usuwane; ostrzeżenie (`[uwaga] znaki spoza słownika Kokoro zostaną pominięte`) pojawia się raz na znak.
  Białe znaki są najpierw ujednolicane do pojedynczej spacji.
- **Porcje (6).** Limit liczony w code pointach (tak liczy Kokoro). Cięcie: najpierw po `, ; : — …`, potem po
  spacjach, na końcu na twardo. Porcje krótszych fragmentów są sklejane z powrotem do limitu.
- **Inferencja (7).** Wejścia modelu: `input_ids` (int64, `[1, n+2]`, z BOS/EOS = 0), `style` (float32, `[1, 256]`,
  wiersz `n−1` macierzy 510×256, gdzie `n` to liczba fonemów porcji), `speed` (float32, `[1]`); wyjście: `waveform`.
  Nazwy wejść i wyjść są sprawdzane przy ładowaniu.
- **Sklejanie (8).** Porcje układane są jedna za drugą z ciszą (`float32` zer) o długości pauzy.

## 5. Model lokalnie

Pliki leżą w `<model_dir>/<REVISION>/…` (`REVISION` = `v2.1.1`): `catalog.json`, plik tokenizera, głos (`voices/….bin`,
510×256 `float32` little-endian) i model `.onnx` (ścieżki z katalogu). Które pliki — wynika z języka mówcy
(`Config::lang`, domyślnie `pl`) i głosu (`Config::voice`, domyślnie `defaultVoiceId` języka): głos wskazuje model
(`modelId`), model — tokenizer. Pobierane są tylko pliki wybranego głosu. Katalog tylko do odczytu (pakiet Nix
w `/nix/store`) bez wybranego głosu daje błąd z podpowiedzią zamiast `Permission denied`. Katalog domyślny:
`KOKORO_MODEL_DIR` albo `<katalog cache użytkownika>/kokoro-pl/kokoro-kmp-models`. **Układ jest wspólny z wersją Pythona.**

Kolejność szukania pliku: (1) jest na dysku i niepusty → zero sieci; (2) `offline` → błąd z instrukcją; (3) pobranie
`GET <endpoint>/<repo>/resolve/<rewizja>/<ścieżka>` do `<plik>.part` i atomowe `rename`. Pusty lub przerwany plik
jest pobierany ponownie; niezgodny rozmiar (`Content-Length`) to błąd bez pliku końcowego. Ścieżki z `catalog.json`
muszą być lokalne (`..` i ścieżki bezwzględne są odrzucane). Endpoint: `HF_ENDPOINT`, token: `HF_TOKEN`. Klient HTTP (`ureq`) nie ma domyślnych limitów czasu, więc biblioteka ustawia
limit połączenia (30 s) i odpowiedzi nagłówkowej (60 s); ciało (setki MB) nie ma limitu całkowitego, a jest odczytywane
w porcjach 64 KiB ze sprawdzaniem `CancelToken`. Odpowiedź urwana przed `Content-Length` daje błąd bez śladu na dysku.

## 6. Cykl życia i pamięć

`load_model`: (1) backend G2P — pierwszy, żeby błąd konfiguracji wyszedł przed pobieraniem kilkuset MB;
(2) katalog, słownik, styl; (3) plik modelu; (4) sesja ONNX. Błąd na dowolnym etapie zamyka G2P.

`Model::unload` (idempotentne): czeka na trwające wywołania (blokada wyłączna `RwLock`), niszczy sesję ONNX, zamyka
G2P, czyści cache, na Linuxie z glibc wywołuje `malloc_trim(0)`. `Drop` robi to samo. Po `unload` każda operacja
zwraca `Error::Unloaded`.

**Środowisko ONNX Runtime jest jedno na proces i nie da się go zniszczyć** (ograniczenie crate'a `ort`). Skutki:

- `unload` zwalnia **sesję** (wagi modelu), ale `libonnxruntime.so` i jej globalny stan zostają w pamięci do końca
  procesu. Pomiar z atrapą modelu (przykład `memory`): RSS po `unload` spada o ok. 1 MB (27,0 → 26,0 MB na ORT 1.22.0),
  podczas gdy wersja Go zwalniała bibliotekę (30,5 → 11,7 MB). Z prawdziwym Kokoro zwolnienie wag powinno być
  wyraźnie większe, ale **nie zmierzyłem tego** (atrapa ma kilka KB wag).
- Wszystkie modele w procesie muszą używać tej samej `libonnxruntime.so`; inna ścieżka daje czytelny błąd.
- Jeśli musisz oddać całą pamięć ORT, uruchamiaj syntezę w osobnym procesie (np. CLI).

**Zamykanie procesu.** `ort` rejestruje destruktor zwalniający środowisko przy wyjściu, a ORT 1.21/1.22 kończy się
przy tym SIGSEGV-em (kod 139) już po poprawnej pracy (ślad gdb: `ReleaseEnv` wewnątrz `libonnxruntime.so`; 1.23+ jest
w porządku). Biblioteka trzyma dlatego jedno nigdy nie zwalniane odwołanie do środowiska (`engine.rs`, `ensure_ort`),
co usuwa awarię (sprawdzone na 1.21.0, 1.22.0, 1.23.2, 1.30.0, także w kompilacji release). To obejście, nie elegancki
wzorzec: autorzy `ort` zastrzegają, że ORT jest wrażliwy na kolejność zwalniania.

**Współbieżność.** `Model` jest `Send + Sync`. Inferencje są serializowane na jednej sesji (`Mutex<Session>`);
fonemizacja jest równoległa (wątki + procesy).

**Anulowanie.** `CancelToken` (z `Config::cancel`, `Model::cancel_token()`): `cancel()` zabija podprocesy fonemizacji,
zatrzymuje syntezę między porcjami i przerywa pobieranie między porcjami danych; zwracany jest `Error::Cancelled`.
Pojedynczej inferencji ani zablokowanego odczytu z sieci nie da się przerwać.

## 7. Decyzje i kompromisy

| Decyzja | Powód / skutek |
|---|---|
| Phonemis przez podproces (`phonemis_runner`) | brak bindingów; nie wymaga linkowania C++; koszt: start procesu i ładowanie wag na każdy kawałek (łagodzone równoległością i cache) |
| Ręczne granice zamiast lookaroundów | crate `regex` ich nie ma; zgodność z Pythonem pilnuje `tests/parity.rs` (15 565 przypadków, te same co w Go) |
| ORT przez crate `ort =2.0.0-rc.13`, `load-dynamic` | brak pobierania/linkowania ORT w czasie budowania; wersja przypięta, bo API `ort` zmienia się między wydaniami rc |
| Cecha `api-21`, nie `api-22` | z `api-22` `ort` ustawia automatyczny wybór providerów, a ORT 1.22.0 wywala się w tej ścieżce asercją C++ (lista urządzeń pusta); `api-21` jej nie wywołuje, obniża minimum ORT do 1.21; test cyklu życia na ORT 1.23.2/1.30.0 trwał z `api-22` 1,0–2,4 s, z `api-21` ok. 0,08 s (przyczynę — pewnie odpytywanie urządzeń przy automatycznym wyborze — wnioskuję, nie mierzyłem osobno) |
| Providery poza CPU za cechami Cargo | `cuda`, `tensorrt`, `rocm`, `migraphx`, `coreml`, `directml`, `openvino`; rejestrowane z `error_on_failure` (brak providera = błąd, nie ciche CPU). Kompilują się, brak providera w ORT daje błąd (sprawdzone dla MIGraphX); **na prawdziwym GPU nie uruchamiane** |
| `SynthOptions: Default` | brak pułapki z Go (struktura podana wprost zerowała pauzy); pauza `Duration::ZERO` znaczy brak pauzy |
| Lokalna kopia modelu jako jedyne źródło | powtarzalność i praca offline; brak kopiowania z globalnego cache `huggingface_hub` |
| Obejście SIGSEGV przy wyjściu | patrz sekcja 6 |

## 8. Znane ograniczenia

- Odmiana liczebników tylko w mianowniku (Phonemis); daty słowne i lata — patrz 3.2.
- G2P działa na zdaniach bez kontekstu międzywyrazowego i bez rozróżniania homografów.
- Normalizacja to heurystyki; `zł` z ułamkiem zawsze „złotych”.
- Nie ma strumieniowania: `synthesize` zwraca całość; anulowanie działa między porcjami.
- `unload` nie oddaje pamięci samej biblioteki ORT (sekcja 6).
- Runner jest zlinkowany dynamicznie; zbudowany przez `nix/phonemis.nix` zależy od ścieżek w `/nix/store` ([devenv.md](devenv.md)).
- Normalizacja uzupełniająca istnieje tylko dla polskiego; inne języki polegają wyłącznie na Phonemis.
- Wybór `\w`/`\d` w granicach: litera/liczba Unicode (`\p{L}`, `\p{N}`, `_`) jak w Pythonie; cyfry w wyrażeniach tylko ASCII.
