//! Pomocnicze funkcje tekstowe. Crate `regex` (jak RE2 w Go) nie ma lookaroundów, więc granice są sprawdzane
//! ręcznie; zachowanie odpowiada wersji Pythona (`\w` = litera/cyfra/podkreślnik, `\s` = biały znak Unicode).

use std::sync::LazyLock;

use regex::{Captures, Regex};

static WORD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\p{L}\p{N}_]$").unwrap());
static ND_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\p{Nd}$").unwrap());

/// Odpowiednik `\w` z Pythona: litera (L*), liczba (N*) albo `_`.
pub(crate) fn is_word(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric() || c == '_';
    }
    let mut b = [0u8; 4];
    WORD_RE.is_match(c.encode_utf8(&mut b))
}

/// Odpowiednik `\d` z Pythona w lookaroundach: cyfra dziesiętna Unicode (Nd).
pub(crate) fn is_digit(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_digit();
    }
    let mut b = [0u8; 4];
    ND_RE.is_match(c.encode_utf8(&mut b))
}

pub(crate) fn prev_char(s: &str, i: usize) -> Option<char> {
    s[..i].chars().next_back()
}

pub(crate) fn next_char(s: &str, i: usize) -> Option<char> {
    s[i..].chars().next()
}

/// `re.sub(r"\s+", " ", s).strip()`
pub(crate) fn collapse_spaces(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Zamienia dopasowania `re`, o ile `f` zwróci `Some(zamiennik)` (`f` sprawdza granice, czyli to, co w Pythonie
/// robiły lookaroundy). Gdy `f` odrzuci kandydata, szukamy dalej od NASTĘPNEGO znaku jego początku — tak jak silnik
/// Pythona po nieudanym lookbehind — a nie od końca odrzuconego dopasowania; inaczej ginęłyby dopasowania
/// zaczynające się wewnątrz niego (np. „000%" w „tzn.1 000%").
pub(crate) fn replace_matches<F>(re: &Regex, s: &str, mut f: F) -> String
where
    F: FnMut(&Captures) -> Option<String>,
{
    let mut out = String::with_capacity(s.len());
    let (mut last, mut pos) = (0usize, 0usize);
    while pos <= s.len() {
        let Some(caps) = re.captures_at(s, pos) else { break };
        let m = caps.get(0).unwrap();
        let step = s[m.start()..].chars().next().map_or(1, char::len_utf8);
        match f(&caps) {
            None => pos = m.start() + step,
            Some(rep) => {
                out.push_str(&s[last..m.start()]);
                out.push_str(&rep);
                last = m.end();
                pos = m.end();
                if m.end() == m.start() {
                    pos += step; // dopasowanie puste — unikamy pętli nieskończonej
                }
            }
        }
    }
    out.push_str(&s[last..]);
    out
}

// ---------------------------------------------------------------------------
// Dzielenie na akapity i zdania
// ---------------------------------------------------------------------------

const UPPER_PL: &str = "ĄĆĘŁŃÓŚŹŻ";

fn is_upper_pl(c: char) -> bool {
    c.is_ascii_uppercase() || UPPER_PL.contains(c)
}

/// Znacznik ręcznego IPA otwierający zdanie (patrz `overrides`).
pub(crate) const PH_OPEN: char = '\u{E000}';

/// Akapity -> zdania. Zdanie zaczynamy tylko przed wielką literą (mniej fałszywych cięć, np. po „np. kot"); znacznik
/// ręcznego IPA też otwiera zdanie.
pub fn split_sentences(text: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        let sents: Vec<String> =
            split_sentence_boundaries(para).into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        out.push(sents);
    }
    out
}

fn split_sentence_boundaries(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut start, mut i) = (0usize, 0usize);
    while i < s.len() {
        let c = s[i..].chars().next().unwrap();
        if !c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        let mut j = i;
        while j < s.len() {
            let c2 = s[j..].chars().next().unwrap();
            if !c2.is_whitespace() {
                break;
            }
            j += c2.len_utf8();
        }
        if sentence_break_before(&s[..i]) && sentence_start_after(&s[j..]) {
            parts.push(&s[start..i]);
            start = j;
        }
        i = j;
    }
    parts.push(&s[start..]);
    parts
}

fn sentence_break_before(prefix: &str) -> bool {
    let Some(last) = prefix.chars().next_back() else { return false };
    if ".!?…".contains(last) {
        return true;
    }
    if "\"”»)".contains(last) {
        return prefix[..prefix.len() - last.len_utf8()].chars().next_back().is_some_and(|b| ".!?…".contains(b));
    }
    false
}

fn sentence_start_after(rest: &str) -> bool {
    let mut it = rest.chars();
    let Some(mut c) = it.next() else { return false };
    if "\"„“(«".contains(c) {
        match it.next() {
            Some(n) => c = n,
            None => return false,
        }
    }
    is_upper_pl(c) || c == PH_OPEN
}

// ---------------------------------------------------------------------------
// Cięcie IPA na porcje (limit w code pointach — tak liczy Kokoro)
// ---------------------------------------------------------------------------

/// Tnie po `, ; : — …` i połyka spacje za nimi.
fn split_after_clause(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        cur.push(c);
        if ",;:—…".contains(c) {
            parts.push(std::mem::take(&mut cur));
            while it.peek().is_some_and(|n| n.is_whitespace()) {
                it.next();
            }
        }
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
}

/// Tnie IPA na porcje `<= limit`: najpierw po interpunkcji, potem po spacjach, na końcu na twardo.
pub fn pack_ipa(ipa: &str, limit: usize) -> Vec<String> {
    pack_ipa_level(ipa, limit, 0)
}

fn pack_ipa_level(ipa: &str, limit: usize, level: usize) -> Vec<String> {
    let ipa = ipa.trim();
    let len = ipa.chars().count();
    if len <= limit {
        return if ipa.is_empty() { Vec::new() } else { vec![ipa.to_string()] };
    }
    if level >= 2 {
        let rs: Vec<char> = ipa.chars().collect();
        return rs.chunks(limit).map(|c| c.iter().collect()).collect();
    }
    let raw: Vec<String> = if level == 0 { split_after_clause(ipa) } else { ipa.split_whitespace().map(str::to_string).collect() };
    let parts: Vec<String> = raw.into_iter().filter(|p| !p.trim().is_empty()).collect();
    if parts.len() == 1 {
        return pack_ipa_level(ipa, limit, level + 1);
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    for part in &parts {
        for piece in pack_ipa_level(part, limit, level + 1) {
            let cand = if cur.is_empty() { piece.clone() } else { format!("{cur} {piece}") };
            if cand.chars().count() <= limit {
                cur = cand;
            } else {
                out.push(std::mem::take(&mut cur));
                cur = piece;
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}
