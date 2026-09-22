//! Zgodność z referencyjną implementacją Pythona (testdata/parity.json, generator: testdata/gen_parity.py).
//! Ten sam korpus sprawdza port Go, więc obie implementacje są porównywane z tym samym wzorcem.

use plkokoro::__internal::{extract_overrides, pack_ipa, sentence_ipa, split_sentences};
use plkokoro::normalize_pl;
use serde::Deserialize;

#[derive(Deserialize)]
struct Norm {
    #[serde(rename = "in")]
    input: String,
    out: String,
}
#[derive(Deserialize)]
struct Split {
    #[serde(rename = "in")]
    input: String,
    out: Vec<Vec<String>>,
}
#[derive(Deserialize)]
struct Pack {
    ipa: String,
    limit: usize,
    out: Vec<String>,
}
#[derive(Deserialize)]
struct Ovr {
    #[serde(rename = "in")]
    input: String,
    normalize: bool,
    overrides: Vec<String>,
    lines: Vec<String>,
}
#[derive(Deserialize)]
struct Parity {
    normalize: Vec<Norm>,
    split: Vec<Split>,
    pack: Vec<Pack>,
    overrides: Vec<Ovr>,
}

fn load() -> Parity {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../testdata/parity.json");
    serde_json::from_slice(&std::fs::read(path).expect("brak testdata/parity.json")).expect("zły parity.json")
}

fn report(name: &str, bad: usize, total: usize) {
    eprintln!("{name}: {}/{total} zgodnych z Pythonem", total - bad);
    assert_eq!(bad, 0, "{name}: {bad} niezgodności");
}

#[test]
fn parity_normalize() {
    let d = load();
    let mut bad = 0;
    for c in &d.normalize {
        let got = normalize_pl(&c.input);
        if got != c.out {
            bad += 1;
            if bad <= 8 {
                eprintln!("normalize({:?})\n  rs: {:?}\n  py: {:?}", c.input, got, c.out);
            }
        }
    }
    report("normalize", bad, d.normalize.len());
}

#[test]
fn parity_split_sentences() {
    let d = load();
    let mut bad = 0;
    for c in &d.split {
        let got = split_sentences(&c.input);
        if got != c.out {
            bad += 1;
            if bad <= 8 {
                eprintln!("split({:?})\n  rs: {:?}\n  py: {:?}", c.input, got, c.out);
            }
        }
    }
    report("split", bad, d.split.len());
}

#[test]
fn parity_pack_ipa() {
    let d = load();
    let mut bad = 0;
    for c in &d.pack {
        let got = pack_ipa(&c.ipa, c.limit);
        if got != c.out {
            bad += 1;
            if bad <= 8 {
                eprintln!("pack({:?}, {})\n  rs: {:?}\n  py: {:?}", c.ipa, c.limit, got, c.out);
            }
        }
    }
    report("pack", bad, d.pack.len());
}

#[test]
fn parity_overrides() {
    let d = load();
    let mut bad = 0;
    for c in &d.overrides {
        let (marked, ovr) = extract_overrides(&c.input);
        let norm = if c.normalize { normalize_pl(&marked) } else { marked };
        let lines: Vec<String> = split_sentences(&norm).iter().flatten().map(|s| sentence_ipa(s, &ovr, &|t| format!("<{t}>"))).collect();
        if lines != c.lines || ovr != c.overrides {
            bad += 1;
            if bad <= 8 {
                eprintln!(
                    "overrides({:?}, norm={})\n  rs: {:?} {:?}\n  py: {:?} {:?}",
                    c.input, c.normalize, lines, ovr, c.lines, c.overrides
                );
            }
        }
    }
    report("overrides", bad, d.overrides.len());
}
