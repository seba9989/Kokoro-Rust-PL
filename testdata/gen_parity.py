#!/usr/bin/env python3
"""Generuje testdata/parity.json z referencyjnej implementacji Pythona (pl_kokoro_tts.py).
Użycie: python testdata/gen_parity.py /ścieżka/do/pl_kokoro_tts.py_katalog > testdata/parity.json"""
import json, random, sys
sys.path.insert(0, sys.argv[1])
import pl_kokoro_tts as m

rnd = random.Random(20260921)

NORM_TOK = ["0", "1", "2", "5", "7", "12", "22", "30", "123", "00", "1 000", "12 345", "3,5", "7.25", " ", "  ", "\t", "\n",
            ".", ",", ":", ";", "zł", "PLN", "%", "&", "godz.", "np.", "ok.", "itd.", "itp.", "tzn.", "tel.", "tzw.", "ul.",
            "a", "Ala", "ma", "kota", "ó", "Ł", "„", "”", "“", "«", "»", "'", "’", "‘", "–", "—", "\u00a0", "-", "(", ")", "!", "?", "…",
            "12:30", "9:05", "24:00", "23:59", "1:2:3", "x1", "_5", "zło", "złotych", "PLNx", "5zł", "5 zł", "50%", "5 %", "x%"]
NORM_HAND = ["Mam 123 zł i 5% rabatu.", "Kosztuje 1 zł, 2 zł, 5 zł, 22 zł, 12 zł, 1 000 zł.", "Start o godz. 12:30, koniec 18:00, np. w piątek.",
             "Ala & Ola: „cześć” – powiedziały.", "3,5 zł i 7,5%", "Trwa ok. 5 minut.", "Wszystko jest ok.", "Ok. Dobra.", "tel. 600 123 456, itd.",
             "1.500 zł", "a1,500 zł", "3.1.5 zł", "123:30", "1:2:3", "zł", " zł", "5 złotych", "godz.", "godz. abc", "godz.12", "ok.7", "xok. 5", "xgodz. 5",
             "np.", "np.x", "npx.", "5 PLN.", "5PLN", "0 zł", "001 zł", "1 zł", "101 zł", "111 zł", "112 zł", "1234567890123456789012345 zł", "", "   ", "\n\n"]

SPLIT_TOK = ["Ala", "kot", "Ów", "ala", "Żaba", "ćma", ".", "!", "?", "…", "\"", "”", "»", ")", "(", "„", "“", "«", " ", "  ", "\n", "\n\n",
             "\ue000", "\ue001", "\ue100", "np.", "1", "-", ",", "\u00a0", "Ą"]
SPLIT_HAND = ["Pierwsze zdanie. Drugie zdanie? \"Trzecie!\" Czwarte, np. kot. Piąte.\n\nNowy akapit. Koniec.", "A. B. C.", "a. b. c.", "Tak.\"Nie", "Tak.\" Nie",
              "Tak.) Nie", "Tak…„Nie", "Tak… „Nie”", "", "  ", "x. \ue000\ue100\ue001 y", "Koniec. (Nowe) zdanie.", "Koniec. ( nowe) zdanie."]

IPA_LET = "abɛɔɕʑʒʂʐɲŋɨəɹˈ"
IPA_SEP = [" ", ", ", ",", ";", ": ", "—", "… ", ". ", "  ", " ,", "?", "!"]
def rand_ipa():
    n = rnd.randint(0, 40)
    return "".join("".join(rnd.choice(IPA_LET) for _ in range(rnd.randint(1, 9))) + rnd.choice(IPA_SEP) for _ in range(n))

OVR_TOK = ["Ala", "kot", "Ala.", "Kot", " ", "  ", "\n", ".", ",", "!", "np.", "5", "zł", "%", "12:30", "[X](/kɔkɔ/)", "[Y](/a:b 12:30 5% & zł/)", "[Z](//)",
           "[Q](/  /)", "[W](/ab\ncd/)", "[", "]", "(/", "/)", "[a]", "(/x/)", "[b](/y/)[c](/z/)", "„", "”", "–", "\u00a0", "Zdanie.", "Nowe"]
OVR_HAND = ["Lubię [Kokoro](/kˈOkəɹO/).", "Test [x](/a:b 12:30 5% & zł/) ok.", "[A](/aa/)[B](/bb/) i [C](/cc/), [D](/dd/)!", "Pierwsze zdanie. [Kokoro](/kk/) drugie.",
            "Słowo [Kokoro](//) i [inne] oraz (/x/) zostają.", "[Kokoro](/kk/)", "ala [Kokoro](/kk/) ma"]

def gen(tokens, hand, n, maxlen=12):
    out = list(hand)
    for _ in range(n):
        out.append("".join(rnd.choice(tokens) for _ in range(rnd.randint(1, maxlen))))
    return out

data = {"normalize": [], "split": [], "pack": [], "overrides": []}
for s in gen(NORM_TOK, NORM_HAND, 3500):
    data["normalize"].append({"in": s, "out": m.normalize_pl(s)})
for s in gen(SPLIT_TOK, SPLIT_HAND, 3500, 16):
    data["split"].append({"in": s, "out": m.split_sentences(s)})
for _ in range(2500):
    ipa, lim = rand_ipa(), rnd.choice([1, 2, 3, 5, 10, 20, 60, 300])
    data["pack"].append({"ipa": ipa, "limit": lim, "out": m.pack_ipa(ipa, lim)})
for s in gen(OVR_TOK, OVR_HAND, 3000, 10):
    for normalize in (True, False):
        marked, ovr = m.extract_overrides(s)
        norm = m.normalize_pl(marked) if normalize else marked
        lines = [m.sentence_ipa(x, ovr, lambda t: "<" + t + ">") for para in m.split_sentences(norm) for x in para]
        data["overrides"].append({"in": s, "normalize": normalize, "overrides": ovr, "lines": lines})
json.dump(data, sys.stdout, ensure_ascii=False)
