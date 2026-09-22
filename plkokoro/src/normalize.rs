//! Normalizacja uzupełniająca (zł, %, &, skróty, godziny, cudzysłowy), wykonywana PRZED Phonemis.
//! Phonemis sam zamienia liczby na słowa (mianownik). W słowniku skrótów są tylko rozwinięcia nieodmienne:
//! „ul." czy „tzw." wymagałyby rodzaju/przypadku.

use std::sync::LazyLock;

use regex::{Captures, Regex};

use crate::text::{is_digit, is_word, next_char, prev_char, replace_matches};

// Uwaga: cyfry w wyrażeniach to tylko ASCII ([0-9]) — jak w wersji Go; Python `\d` łapie też np. cyfry arabsko-indyjskie.
const AMOUNT: &str = r"[0-9]{1,3}(?:[ \x{a0}][0-9]{3})+|[0-9]+";

static ABBR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"np\.|itd\.|itp\.|tzn\.|tel\.").unwrap());
static GODZ_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"godz\.\s*").unwrap());
static OK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"ok\.\s*").unwrap());
static ZL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"({AMOUNT})([.,][0-9]+)?\s*(?:zł|PLN)")).unwrap());
static PCT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"({AMOUNT})([.,][0-9]+)?\s*%")).unwrap());
static TIME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([01]?[0-9]|2[0-3]):([0-5][0-9])").unwrap());
static TABS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]+").unwrap());

fn abbrev(a: &str) -> &'static str {
    match a {
        "np." => "na przykład",
        "itd." => "i tak dalej",
        "itp." => "i tym podobne",
        "tzn." => "to znaczy",
        "tel." => "telefon",
        _ => "",
    }
}

fn map_char(c: char) -> char {
    match c {
        '„' | '”' | '“' | '«' | '»' => '"',
        '’' | '‘' => '\'',
        '–' => '—',
        '\u{a0}' => ' ',
        c => c,
    }
}

/// Forma „złoty/złote/złotych"; `digits` to same cyfry (bez limitu długości).
fn zl_form(digits: &str, has_fraction: bool) -> &'static str {
    if has_fraction {
        return "złotych"; // „3,5 zł" — zostaje przybliżenie
    }
    if digits.trim_start_matches('0') == "1" {
        return "złoty";
    }
    let tail = if digits.len() > 2 { &digits[digits.len() - 2..] } else { digits };
    let n100: u32 = tail.parse().unwrap_or(0);
    if (12..=14).contains(&n100) {
        return "złotych";
    }
    match n100 % 10 {
        2..=4 => "złote",
        _ => "złotych",
    }
}

/// Odpowiednik lookbehind `(?<![\d.,])` (albo `(?<![\d:])` dla godzin): poprzedni znak nie jest cyfrą ani z `set`.
fn not_preceded_by(s: &str, i: usize, set: &str) -> bool {
    prev_char(s, i).is_none_or(|c| !(is_digit(c) || set.contains(c)))
}

fn not_followed_by(s: &str, i: usize, set: &str) -> bool {
    next_char(s, i).is_none_or(|c| !(is_digit(c) || set.contains(c)))
}

fn only_digits(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

fn amount_and_fraction(caps: &Captures) -> (String, String) {
    let digits = only_digits(caps.get(1).unwrap().as_str()); // „1 000" -> „1000" (Phonemis czyta grupy osobno)
    let frac = caps.get(2).map_or(String::new(), |m| m.as_str().to_string());
    (digits, frac)
}

/// Normalizacja uzupełniająca. Patrz dokumentacja modułu.
pub fn normalize_pl(text: &str) -> String {
    let text: String = text.chars().map(map_char).collect();

    // (?<!\w)godz\.\s*(?=\d): „o godz. 12:30" -> „o 12:30"
    let text = replace_matches(&GODZ_RE, &text, |c| {
        let m = c.get(0).unwrap();
        if prev_char(&text, m.start()).is_some_and(is_word) || !next_char(&text, m.end()).is_some_and(is_digit) {
            return None;
        }
        Some(String::new())
    });
    // (?<!\w)ok\.\s*(?=\d): „ok. 5 minut" -> „około 5 minut"; samo „ok." zostaje
    let text = replace_matches(&OK_RE, &text, |c| {
        let m = c.get(0).unwrap();
        if prev_char(&text, m.start()).is_some_and(is_word) || !next_char(&text, m.end()).is_some_and(is_digit) {
            return None;
        }
        Some("około ".to_string())
    });
    // (?<!\w)(np\.|…)(?=\s|$)
    let text = replace_matches(&ABBR_RE, &text, |c| {
        let m = c.get(0).unwrap();
        if prev_char(&text, m.start()).is_some_and(is_word) || next_char(&text, m.end()).is_some_and(|n| !n.is_whitespace()) {
            return None;
        }
        Some(abbrev(m.as_str()).to_string())
    });
    // (?<![\d.,])(kwota)(ułamek)?\s*(zł|PLN)(?!\w)
    let text = replace_matches(&ZL_RE, &text, |c| {
        let m = c.get(0).unwrap();
        if !not_preceded_by(&text, m.start(), ".,") || next_char(&text, m.end()).is_some_and(is_word) {
            return None;
        }
        let (digits, frac) = amount_and_fraction(c);
        Some(format!("{digits}{frac} {}", zl_form(&digits, !frac.is_empty())))
    });
    let text = replace_matches(&PCT_RE, &text, |c| {
        let m = c.get(0).unwrap();
        if !not_preceded_by(&text, m.start(), ".,") {
            return None;
        }
        let (digits, frac) = amount_and_fraction(c);
        Some(format!("{digits}{frac} procent"))
    });
    // (?<![\d:])(godz):(min)(?![\d:])
    let text = replace_matches(&TIME_RE, &text, |c| {
        let m = c.get(0).unwrap();
        if !not_preceded_by(&text, m.start(), ":") || !not_followed_by(&text, m.end(), ":") {
            return None;
        }
        let h = c.get(1).unwrap().as_str();
        let mi: u32 = c.get(2).unwrap().as_str().parse().unwrap_or(0);
        Some(if mi == 0 { h.to_string() } else { format!("{h} {mi}") })
    });
    let text = text.replace('&', " i ");
    TABS_RE.replace_all(&text, " ").trim().to_string()
}
