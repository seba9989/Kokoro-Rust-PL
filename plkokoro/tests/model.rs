//! Testy modelu na PRAWDZIWEJ sesji ONNX Runtime z modelem-atrapą (testdata/tiny_kokoro.onnx), fałszywym serwerem HF
//! i fałszywym runnerem. Wymagają ORT_LIBRARY_PATH (bez niego wypisują SKIP i kończą się od razu).

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::*;
use plkokoro::{load_model, CancelToken, Config, Error, G2p, SynthOptions, TextOptions, SAMPLE_RATE};

/// Liczy to, co robi tiny_kokoro.onnx: 100 próbek na token wejściowy (razem z BOS/EOS), każda = sum(input_ids) +
/// sum(styl)*speed. Sprawdza więc ids, wiersz stylu (n-1) i speed.
fn expected_chunk(ipa: &str, speed: f32) -> (usize, f32) {
    let chars: Vec<char> = ipa.chars().collect();
    let sum_ids: f32 = chars.iter().map(|&c| vocab_id(c) as f32).sum();
    let sum_style: f32 = (0..256).map(|j| style_val(chars.len() - 1, j)).sum();
    ((chars.len() + 2) * 100, sum_ids + sum_style * speed)
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3 * b.abs().max(1.0)
}

fn secs(d: Duration) -> usize {
    (f64::from(SAMPLE_RATE) * d.as_secs_f64()) as usize
}

#[test]
fn lifecycle_real_ort() {
    let ort = require_ort!();
    logs::init();
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    let model = load_model(base_cfg(tmp.path(), &hf.url, &ort)).unwrap();
    assert_eq!(hf.count(), 5, "catalog, cfg, głos, model (+1 przekierowanie CDN)");

    // 2. tekst -> IPA: zdania w liniach, akapity oddzielone pustą linią
    let ipa = model.text_to_ipa("Ala ma kota. Kot ma ale!\n\nNowy akapit.", &TextOptions::default()).unwrap();
    assert_eq!(ipa, "ala ma kota.\nkot ma ale!\n\nnowy akapit.");
    assert_eq!(model.text_to_ipa("  \n ", &TextOptions::default()).unwrap(), "");

    // 3. IPA -> audio: długość i wartości próbek z prawdziwej sesji ORT
    let speed = 1.25;
    let opts = SynthOptions { speed, ..Default::default() };
    let wav = model.synthesize(&ipa, &opts).unwrap();
    let lines = ["ala ma kota.", "kot ma ale!", "nowy akapit."];
    let pauses = [opts.sentence_pause, opts.paragraph_pause, Duration::ZERO]; // 2. zdanie kończy akapit; po ostatniej porcji nic
    let (mut pos, mut want_len) = (0usize, 0usize);
    for (i, l) in lines.iter().enumerate() {
        let (n, val) = expected_chunk(l, speed);
        for k in 0..n {
            assert!(
                approx(wav[pos + k], val),
                "linia {i} próbka {k} = {}, oczekiwano {val} (ids/styl/speed przekazane błędnie?)",
                wav[pos + k]
            );
        }
        pos += n;
        want_len += n;
        let z = secs(pauses[i]);
        assert!(wav[pos..pos + z].iter().all(|&x| x == 0.0), "pauza po linii {i} nie jest ciszą");
        pos += z;
        want_len += z;
    }
    assert_eq!(wav.len(), want_len);

    // ręczne IPA + filtr słownika (§ nie ma w słowniku: ostrzeżenie raz, nie błąd)
    let ipa2 = model.text_to_ipa("Lubie [Kokoro](/kokoʒ§/) bardzo.", &TextOptions::default()).unwrap();
    assert!(ipa2.contains("kokoʒ§") && !ipa2.contains("kokoro"), "ręczne IPA: {ipa2:?}");
    model.synthesize(&ipa2, &SynthOptions::default()).unwrap();
    model.synthesize(&ipa2, &SynthOptions::default()).unwrap();
    assert_eq!(logs::count_containing("§"), 1, "ostrzeżenie o znaku spoza słownika ma być raz");

    // 4. unload: idempotentny, potem Unloaded
    model.unload().unwrap();
    model.unload().unwrap();
    assert!(!model.is_loaded());
    assert!(matches!(model.text_to_ipa("ala", &TextOptions::default()), Err(Error::Unloaded)));
    assert!(matches!(model.synthesize("ala", &SynthOptions::default()), Err(Error::Unloaded)));
}

#[test]
fn reload_and_two_models() {
    let ort = require_ort!();
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();

    let a = load_model(base_cfg(tmp.path(), &hf.url, &ort)).unwrap();
    let reqs = hf.count();
    let b = load_model(base_cfg(tmp.path(), &hf.url, &ort)).unwrap(); // drugi model równolegle
    assert_eq!(hf.count(), reqs, "ponowne ładowanie użyło sieci");
    a.unload().unwrap(); // niszczy sesję A; B ma dalej działać
    let ipa = b.text_to_ipa("Ala ma kota.", &TextOptions::default()).unwrap();
    b.synthesize(&ipa, &SynthOptions::default()).expect("model B po zwolnieniu A");
    b.unload().unwrap();

    // wszystko zwolnione — ładowanie od nowa musi działać
    let c = load_model(base_cfg(tmp.path(), &hf.url, &ort)).expect("ponowne ładowanie po zwolnieniu wszystkich");
    let ipa = c.text_to_ipa("Ala ma kota.", &TextOptions::default()).unwrap();
    assert!(!c.synthesize(&ipa, &SynthOptions::default()).unwrap().is_empty());
}

#[test]
fn options_validation_and_errors() {
    let ort = require_ort!();
    logs::init();
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    let model = load_model(base_cfg(tmp.path(), &hf.url, &ort)).unwrap();

    for speed in [0.0, -1.0, f32::NAN] {
        let r = model.synthesize("ala", &SynthOptions { speed, ..Default::default() });
        assert!(matches!(r, Err(Error::InvalidOption(_))), "speed={speed} powinno dać InvalidOption: {r:?}");
    }
    for input in ["", "   ", "\n\n", "@@@", "%%%"] {
        assert!(matches!(model.synthesize(input, &SynthOptions::default()), Err(Error::NoPhonemes)), "wejście {input:?}");
    }
    match model.text_to_ipa("BOOM", &TextOptions::default()) {
        Err(Error::Runner(m)) => assert!(m.contains("kodem 1") && m.contains("zepsuty model"), "{m}"),
        other => panic!("błąd runnera powinien wyjść z text_to_ipa: {other:?}"),
    }
    // max_phonemes: długie IPA cięte na porcje <= limit; twardo <= 510 (atrapa modelu wyrzuciłaby błąd dla n > 510)
    let long = "abc, ".repeat(300);
    for lim in [60usize, 99_999] {
        let w = model.synthesize(&long, &SynthOptions { max_phonemes: lim, ..Default::default() }).unwrap();
        assert!(!w.is_empty());
    }
    // anulowanie: synthesize zatrzymuje się i zwraca Cancelled
    model.cancel_token().cancel();
    assert!(matches!(model.synthesize("ala ma kota", &SynthOptions::default()), Err(Error::Cancelled)));
}

#[test]
fn synth_options_pause_semantics() {
    let ort = require_ort!();
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    let model = load_model(base_cfg(tmp.path(), &hf.url, &ort)).unwrap();

    let ipa = "ala ma kota.\nkot ma ale."; // dwa zdania jednego akapitu: jedna pauza zdaniowa, po ostatnim nic
    let with_pauses = model.synthesize(ipa, &SynthOptions::default()).unwrap();
    let no_pauses = model
        .synthesize(
            ipa,
            &SynthOptions {
                sentence_pause: Duration::ZERO,
                paragraph_pause: Duration::ZERO,
                clause_pause: Duration::ZERO,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(with_pauses.len() - no_pauses.len(), secs(Duration::from_millis(200)), "różnica = dokładnie jedna pauza zdaniowa");
    // składnia ..Default::default() zachowuje domyślne pauzy przy zmianie samego tempa
    let faster = model.synthesize(ipa, &SynthOptions { speed: 1.5, ..Default::default() }).unwrap();
    assert_eq!(faster.len(), with_pauses.len());
}

#[test]
fn skip_kokoro_uses_no_network() {
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    let (runner, _) = fake_runner(tmp.path());
    let model = load_model(Config {
        model_dir: Some(tmp.path().join("m")),
        hf_endpoint: Some(hf.url.clone()),
        phonemis_runner: Some(runner),
        skip_kokoro: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(hf.count(), 0, "skip_kokoro użył sieci");
    assert_eq!(model.text_to_ipa("Ala ma kota.", &TextOptions::default()).unwrap(), "ala ma kota.");
    assert!(matches!(model.synthesize("ala", &SynthOptions::default()), Err(Error::NoKokoro)));
}

struct Lower {
    seen: Arc<Mutex<Vec<String>>>,
    closed: Arc<AtomicBool>,
}

impl G2p for Lower {
    fn phonemize(&self, text: &str, _: &CancelToken) -> plkokoro::Result<String> {
        self.seen.lock().unwrap().push(text.to_string());
        Ok(text.to_lowercase())
    }
    fn close(&self) -> plkokoro::Result<()> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn custom_g2p_and_cache() {
    let (seen, closed) = (Arc::new(Mutex::new(Vec::new())), Arc::new(AtomicBool::new(false)));
    let model = load_model(Config {
        g2p: Some(Box::new(Lower { seen: seen.clone(), closed: closed.clone() })),
        skip_kokoro: true,
        ..Default::default()
    })
    .unwrap();
    for _ in 0..3 {
        model.text_to_ipa("Lubie [Kokoro](/kk/) bardzo. Ala ma kota.", &TextOptions::default()).unwrap();
    }
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 3, "g2p wołany {} razy ({seen:?}), oczekiwano 3 (cache)", seen.len()); // „Lubie", „bardzo.", „Ala ma kota."
    assert!(seen.iter().all(|s| !s.contains("Kokoro")), "ręczne słowo trafiło do g2p: {seen:?}");
    model.unload().unwrap();
    assert!(closed.load(Ordering::SeqCst), "unload nie zamknął własnego g2p");
}

// Współbieżność: wiele wątków + unload w trakcie; brak paniki, tylko sukces albo Unloaded.
#[test]
fn concurrent_use_and_unload() {
    let ort = require_ort!();
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    let model = load_model(base_cfg(tmp.path(), &hf.url, &ort)).unwrap();
    std::thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                for i in 0..20 {
                    let text = format!("Ala ma kota numer {}. Kot ma ale.", (b'a' + (i % 26) as u8) as char);
                    let r =
                        model.text_to_ipa(&text, &TextOptions::default()).and_then(|ipa| model.synthesize(&ipa, &SynthOptions::default()));
                    match r {
                        Ok(_) | Err(Error::Unloaded) => {}
                        Err(e) => panic!("nieoczekiwany błąd: {e}"),
                    }
                }
            });
        }
        std::thread::sleep(Duration::from_millis(30));
        model.unload().unwrap();
    });
}

/// `skip_kokoro` + `lang`: czyta tylko catalog.json (bez modelu i ORT), więc wybór języka i głosu widać bez sesji ONNX.
fn phonemize_only(
    tmp: &std::path::Path,
    hf: &TestServer,
    lang: &str,
    voice: Option<&str>,
    phonemis_lang: Option<&str>,
) -> plkokoro::Result<plkokoro::Model> {
    let (runner, _) = fake_runner(tmp);
    load_model(Config {
        model_dir: Some(tmp.join("m")),
        hf_endpoint: Some(hf.url.clone()),
        phonemis_runner: Some(runner),
        lang: Some(lang.into()),
        voice: voice.map(Into::into),
        phonemis_lang: phonemis_lang.map(Into::into),
        skip_kokoro: true,
        ..Default::default()
    })
}

#[test]
fn phonemizer_language_follows_speaker_and_can_be_overridden() {
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    let de = fake_weights(tmp.path(), "de");
    fake_weights(tmp.path(), "pt");

    let m = phonemize_only(tmp.path(), &hf, "de", Some("dm_b"), None).unwrap();
    assert_eq!((m.lang(), m.voice(), m.phonemis_lang()), (Some("de"), Some("dm_b"), Some("de")));
    let args = m.text_to_ipa("ARGS", &TextOptions::default()).unwrap();
    assert!(args.starts_with("--lang de --model") && args.ends_with(&de.display().to_string()), "{args}");
    // bez normalizacji PL dla niemieckiego fonemizera
    assert_eq!(m.text_to_ipa("Es kostet 5 zł.", &TextOptions::default()).unwrap(), "es kostet 5 zł.");

    // pt-br -> frontend „pt” z katalogu; głos domyślny języka
    let m = phonemize_only(tmp.path(), &hf, "pt-br", None, None).unwrap();
    assert_eq!((m.voice(), m.phonemis_lang()), (Some("pf_a"), Some("pt")));

    // mówca niemiecki, fonemizer polski (akcent zamierzony)
    let m = phonemize_only(tmp.path(), &hf, "de", None, Some("pl")).unwrap();
    assert_eq!((m.voice(), m.phonemis_lang()), (Some("df_a"), Some("pl")));
    assert_eq!(m.text_to_ipa("Mam 5 zł.", &TextOptions::default()).unwrap(), "mam 5 złotych.");
}

#[test]
fn language_without_frontend_and_bad_selection() {
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();

    // ja: brak frontendu Phonemis -> model bez G2P (runner nie jest nawet wymagany), text_to_ipa z instrukcją
    let m = load_model(Config {
        model_dir: Some(tmp.path().join("m")),
        hf_endpoint: Some(hf.url.clone()),
        lang: Some("ja".into()),
        skip_kokoro: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(m.phonemis_lang(), None);
    let err = m.text_to_ipa("こんにちは", &TextOptions::default()).unwrap_err();
    assert!(err.to_string().contains("--ipa"), "{err}");

    let err = phonemize_only(tmp.path(), &hf, "xx", None, None).err().unwrap().to_string();
    assert!(err.contains("nie znaleziono języka \"xx\"") && err.contains("pt-br"), "{err}");
    let err = phonemize_only(tmp.path(), &hf, "de", Some("nope"), None).err().unwrap().to_string();
    assert!(err.contains("nie ma głosu \"nope\"") && err.contains("df_a, dm_b"), "{err}");
    let err = phonemize_only(tmp.path(), &hf, "pl", Some("dm_b"), None).err().unwrap().to_string();
    assert!(err.contains("należy do języka \"de\""), "{err}");
    // brak wag dla wybranego języka fonemizera
    let err = phonemize_only(tmp.path(), &hf, "de", None, Some("fr")).err().unwrap().to_string();
    assert!(err.contains("brak wag Phonemis") && err.contains("phonemizer_fr.bin"), "{err}");
}

#[test]
fn selected_voice_is_used_for_synthesis() {
    let ort = require_ort!();
    let hf = fake_hf();
    let tmp = tempfile::tempdir().unwrap();
    fake_weights(tmp.path(), "de");
    let model = load_model(Config { lang: Some("de".into()), voice: Some("dm_b".into()), ..base_cfg(tmp.path(), &hf.url, &ort) }).unwrap();
    assert_eq!(model.voice(), Some("dm_b"));
    let base = tmp.path().join("models").join(plkokoro::REVISION);
    assert!(base.join("voices/de_b.bin").is_file() && !base.join("voices/pl.bin").exists(), "pobrano nie ten głos");
    let wav = model.synthesize("abc", &SynthOptions::default()).unwrap();
    assert_eq!(wav.len(), expected_chunk("abc", 1.0).0);

    // ja: synteza z gotowego IPA działa bez fonemizera
    let m = load_model(Config { lang: Some("ja".into()), phonemis_runner: None, ..base_cfg(tmp.path(), &hf.url, &ort) }).unwrap();
    assert!(m.synthesize("abc", &SynthOptions::default()).is_ok());
}
