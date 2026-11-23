#!/usr/bin/env python3
import argparse
import gzip
import hashlib
import json
import os
import re
import sqlite3
import sys
import urllib.request
from multiprocessing import Pool

import gruut_lang_ru
import onnx
from piper.phonemize_espeak import EspeakPhonemizer

VOICE_BASE = "https://huggingface.co/rhasspy/piper-voices/resolve/main/ru/ru_RU/denis/medium"
WORD = re.compile(r"[а-яё]+(-[а-яё]+)*")
DROPPED = {"^"}


def words():
    path = os.path.join(os.path.dirname(gruut_lang_ru.__file__), "lexicon.db")
    rows = sqlite3.connect(path).execute("select distinct word from word_phonemes")
    return sorted(w for (w,) in rows if WORD.fullmatch(w))


def valid(word, tokens, allowed):
    return WORD.fullmatch(word) and all(len(t) == 1 and (t in allowed or t in DROPPED) for t in tokens)


def phonemize(job):
    chunk, allowed = job
    espeak = EspeakPhonemizer()
    done, failed = [], []
    for word in chunk:
        try:
            tokens = [t for sentence in espeak.phonemize("ru", word) for t in sentence]
        except Exception:
            failed.append(word)
            continue
        if tokens and valid(word, tokens, allowed):
            done.append((word, tokens))
        else:
            failed.append(word)
    return done, failed


def generate(ws, allowed, procs):
    entries = {}
    pending = ws
    for size in (300, 40, 5, 1):
        if not pending:
            break
        jobs = [(pending[i : i + size], allowed) for i in range(0, len(pending), size)]
        failed = []
        with Pool(procs, maxtasksperchild=1) as pool:
            for done, bad in pool.imap_unordered(phonemize, jobs):
                entries.update(done)
                failed.extend(bad)
        print(f"chunk {size}: {len(entries)} done, {len(failed)} retry", file=sys.stderr)
        pending = failed
    return list(entries.items())


def fetch(name, dest):
    urllib.request.urlretrieve(f"{VOICE_BASE}/{name}", os.path.join(dest, name))


def prepare_model(work, out):
    config = json.load(open(os.path.join(work, "ru_RU-denis-medium.onnx.json"), encoding="utf-8"))
    model = onnx.load(os.path.join(work, "ru_RU-denis-medium.onnx"))
    while len(model.metadata_props):
        model.metadata_props.pop()
    meta = {
        "model_type": "vits",
        "comment": "piper",
        "language": "Russian",
        "voice": config["espeak"]["voice"],
        "version": 1,
        "has_espeak": 1,
        "has_g2pw": 0,
        "n_speakers": config["num_speakers"],
        "sample_rate": config["audio"]["sample_rate"],
    }
    for key, value in meta.items():
        item = model.metadata_props.add()
        item.key, item.value = key, str(value)
    onnx.save(model, os.path.join(out, "model.onnx"))
    allowed = set()
    with open(os.path.join(out, "tokens.txt"), "w", encoding="utf-8") as f:
        for symbol, ident in config["phoneme_id_map"].items():
            if symbol == "\n" or len(symbol) != 1:
                continue
            ident = ident[0] if isinstance(ident, list) else ident
            f.write(f"{symbol} {ident}\n")
            allowed.add(symbol)
    return allowed


def write_lexicon(entries, allowed, out):
    path = os.path.join(out, "lexicon-ru.txt.gz")
    kept = 0
    with gzip.GzipFile(path, "wb", mtime=0) as gz:
        for word, tokens in sorted(entries):
            phones = "".join(t for t in tokens if t not in DROPPED)
            if phones and all(c in allowed for c in phones):
                gz.write(f"{word}\t{phones}\n".encode("utf-8"))
                kept += 1
    return kept


def licenses(out):
    text = (
        "Voice: ru_RU denis (medium), Piper voice from rhasspy/piper-voices.\n"
        "Dataset: OHF-Voice voice-datasets, license CC0. Base model: rdh (CC0).\n"
        "Model prepared for sherpa-onnx (metadata and tokens.txt, same as sherpa-onnx scripts/piper/add_meta_data.py, Apache-2.0).\n\n"
        "Pronunciation lexicon: word list from gruut-lang-ru (MIT); pronunciations generated once offline with espeak-ng\n"
        "(GPL-3.0-or-later). Only the generated word-to-phoneme data is included; no espeak-ng code or data files are\n"
        "distributed. Whether this data is a derivative work of espeak-ng is undecided.\n"
    )
    open(os.path.join(out, "LICENSES.txt"), "w", encoding="utf-8").write(text)


def digest(out):
    for name in ("model.onnx", "tokens.txt", "lexicon-ru.txt.gz", "LICENSES.txt"):
        data = open(os.path.join(out, name), "rb").read()
        print(f'{hashlib.sha256(data).hexdigest()}  {len(data)}  {name}')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("out")
    parser.add_argument("--raw", help="reuse a previous raw tsv of word<TAB>space separated tokens")
    parser.add_argument("--procs", type=int, default=4)
    args = parser.parse_args()
    os.makedirs(args.out, exist_ok=True)
    work = os.path.join(args.out, ".work")
    os.makedirs(work, exist_ok=True)
    for name in ("ru_RU-denis-medium.onnx", "ru_RU-denis-medium.onnx.json"):
        if not os.path.exists(os.path.join(work, name)):
            fetch(name, work)
    allowed = prepare_model(work, args.out)
    if args.raw:
        entries = [(w, t.split(" ")) for w, t in (l.rstrip("\n").split("\t") for l in open(args.raw, encoding="utf-8") if "\t" in l)]
    else:
        entries = generate(words(), allowed, args.procs)
    kept = write_lexicon(entries, allowed, args.out)
    licenses(args.out)
    print(kept, "words", file=sys.stderr)
    digest(args.out)


if __name__ == "__main__":
    main()
