#!/usr/bin/env python3
import re
import sys

VOWELS = {
    "AA": "ɑː",
    "AE": "æ",
    "AH": "ʌ",
    "AO": "ɔː",
    "AW": "a ʊ",
    "AY": "a ɪ",
    "EH": "ɛ",
    "ER": "ɜː",
    "EY": "e ɪ",
    "IH": "ɪ",
    "IY": "iː",
    "OW": "o ʊ",
    "OY": "ɔ ɪ",
    "UH": "ʊ",
    "UW": "uː",
}
CONSONANTS = {
    "B": "b",
    "CH": "t ʃ",
    "D": "d",
    "DH": "ð",
    "F": "f",
    "G": "ɡ",
    "HH": "h",
    "JH": "d ʒ",
    "K": "k",
    "L": "l",
    "M": "m",
    "N": "n",
    "NG": "ŋ",
    "P": "p",
    "R": "ɹ",
    "S": "s",
    "SH": "ʃ",
    "T": "t",
    "TH": "θ",
    "V": "v",
    "W": "w",
    "Y": "j",
    "Z": "z",
    "ZH": "ʒ",
}
STRESS = {"1": "ˈ", "2": "ˌ"}
WORD = re.compile(r"^[a-z']+$")


def phones(arpa):
    out = []
    for p in arpa:
        m = re.fullmatch(r"([A-Z]+)([012])?", p)
        base, stress = m.group(1), m.group(2)
        if base in CONSONANTS:
            out.append(CONSONANTS[base])
            continue
        if base == "AH" and stress == "0":
            out.append("ə")
        elif base == "ER" and stress == "0":
            out.append("ɚ")
        else:
            if stress in STRESS:
                out.append(STRESS[stress])
            out.append(VOWELS[base])
    return " ".join(out)


VOICELESS = {"p", "t", "k", "f", "θ"}
SIBILANT = {"s", "z", "ʃ", "ʒ"}
VOICELESS_END = VOICELESS | {"s", "ʃ"}


def last_sound(ph):
    toks = ph.split()
    return toks[-1], toks[-2] if len(toks) > 1 else ""


def inflect(word, ph, existing):
    out = {}
    last, prev = last_sound(ph)
    if last == "t" and prev == "ʃ" or (last == "ʒ" and prev == "d"):
        last = "ʃ"
    s_form = " ɪ z" if last in SIBILANT else (" s" if last in VOICELESS else " z")
    out[word + "s"] = ph + s_form
    out[word + "'s"] = ph + s_form
    if word.endswith("e"):
        out[word + "d"] = ph + (" d" if last != "d" and last != "t" else " ɪ d")
    ed = " ɪ d" if last in ("t", "d") else (" t" if last in VOICELESS_END else " d")
    out[word + "ed"] = ph + ed
    stem = word[:-1] if word.endswith("e") else word
    out[stem + "ing"] = ph + " ɪ ŋ"
    return {k: v for k, v in out.items() if k not in existing}


def main(src, dst):
    seen = {}
    with open(src, encoding="utf-8") as f:
        for line in f:
            line = line.split("#")[0].strip()
            if not line:
                continue
            word, *arpa = line.split()
            word = re.sub(r"\(\d+\)$", "", word)
            if not WORD.match(word) or word in seen:
                continue
            seen[word] = phones(arpa)
    derived = {}
    for word, ph in seen.items():
        for k, v in inflect(word, ph, seen).items():
            derived.setdefault(k, v)
    print(len(derived), "derived", file=sys.stderr)
    seen.update(derived)
    with open(dst, "w", encoding="utf-8") as out:
        for word in sorted(seen):
            out.write(f"{word} || {seen[word]}\n")
    print(len(seen), "entries", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
