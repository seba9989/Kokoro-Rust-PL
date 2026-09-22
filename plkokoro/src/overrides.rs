//! Ręczny zapis fonemów: `[tekst](/ipa/)` — składnia jak w Kokoro/misaki, np. „Lubię [Kokoro](/kɔkˈɔrɔ/)."
//! IPA z ukośników trafia do modelu dosłownie, z pominięciem Phonemis. Wyciągamy je PRZED normalizacją (żeby nie
//! ruszyła np. ':' czy cyfr w IPA) i zostawiamy w tekście znacznik z prywatnych znaków Unicode (bez cyfr i liter,
//! więc przeżywa normalizację i dzielenie na zdania).

use std::sync::LazyLock;

use regex::Regex;

use crate::text::{collapse_spaces, replace_matches, PH_OPEN};

pub(crate) const PH_CLOSE: char = '\u{E001}';
const PH_BASE: u32 = 0xE100;
const PH_MAX: usize = (0xF8FF - PH_BASE) as usize;

static OVR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\[\]\n]*)\]\(/([^/\n]*)/\)").unwrap());
static PH_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new("\u{E000}(.)\u{E001}").unwrap());

fn placeholder(idx: usize) -> String {
    let c = char::from_u32(PH_BASE + idx as u32).expect("znacznik w bloku prywatnym");
    format!("{PH_OPEN}{c}{PH_CLOSE}")
}

/// Zamienia `[tekst](/ipa/)` na znaczniki; puste `//` = zwykły tekst. Zwraca (tekst, lista_ipa).
pub fn extract_overrides(text: &str) -> (String, Vec<String>) {
    let mut overrides: Vec<String> = Vec::new();
    let out = replace_matches(&OVR_RE, text, |c| {
        let ipa = c.get(2).unwrap().as_str().trim();
        if ipa.is_empty() || overrides.len() >= PH_MAX {
            return Some(c.get(1).unwrap().as_str().to_string());
        }
        overrides.push(collapse_spaces(ipa));
        Some(placeholder(overrides.len() - 1))
    });
    (out, overrides)
}

/// Kawałki tekstu do fonemizacji (bez znaczników i bez brzegowych spacji).
pub(crate) fn text_pieces(sentence: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut pos = 0;
    for m in PH_RE.find_iter(sentence) {
        let p = sentence[pos..m.start()].trim();
        if !p.is_empty() {
            out.push(p.to_string());
        }
        pos = m.end();
    }
    let p = sentence[pos..].trim();
    if !p.is_empty() {
        out.push(p.to_string());
    }
    out
}

fn override_at<'a>(overrides: &'a [String], marker: &str) -> Option<&'a str> {
    let c = marker.chars().next()?;
    let idx = (c as u32).checked_sub(PH_BASE)? as usize;
    overrides.get(idx).map(String::as_str)
}

/// Składa IPA zdania: kawałki tekstu przez `ipa_of`, znaczniki -> ręczne IPA. Zachowuje pojedynczą spację na styku
/// („Grę [X](/x/) lubię").
pub fn sentence_ipa(sentence: &str, overrides: &[String], ipa_of: &dyn Fn(&str) -> String) -> String {
    let mut parts: Vec<String> = Vec::new();
    let add = |chunk: &str, parts: &mut Vec<String>| {
        if chunk.is_empty() {
            return;
        }
        let core = chunk.trim();
        let ipa = if core.is_empty() { String::new() } else { ipa_of(core) };
        let lead = if chunk.chars().next().is_some_and(char::is_whitespace) { " " } else { "" };
        let trail = if chunk.chars().next_back().is_some_and(char::is_whitespace) { " " } else { "" };
        parts.push(format!("{lead}{ipa}{trail}"));
    };
    let mut pos = 0;
    for caps in PH_RE.captures_iter(sentence) {
        let m = caps.get(0).unwrap();
        add(&sentence[pos..m.start()], &mut parts);
        parts.push(override_at(overrides, caps.get(1).unwrap().as_str()).unwrap_or("").to_string());
        pos = m.end();
    }
    add(&sentence[pos..], &mut parts);
    collapse_spaces(&parts.concat())
}

/// Do logów: znaczniki z powrotem jako `{/ipa/}`.
pub(crate) fn show_overrides(text: &str, overrides: &[String]) -> String {
    PH_RE
        .replace_all(text, |caps: &regex::Captures| match override_at(overrides, caps.get(1).unwrap().as_str()) {
            Some(ipa) => format!("{{/{ipa}/}}"),
            None => String::new(),
        })
        .into_owned()
}
