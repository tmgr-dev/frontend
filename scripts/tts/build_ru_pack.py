#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import urllib.request

import onnx

VOICE_BASE = "https://huggingface.co/rhasspy/piper-voices/resolve/main/ru/ru_RU/denis/medium"


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


def licenses(out):
    text = (
        "Voice: ru_RU denis (medium), Piper voice from rhasspy/piper-voices.\n"
        "Dataset: OHF-Voice voice-datasets, license CC0. Base model: rdh (CC0).\n"
        "Model prepared for sherpa-onnx (metadata and tokens.txt, same as sherpa-onnx scripts/piper/add_meta_data.py, Apache-2.0).\n\n"
        "Pronunciation: rule-based grapheme-to-phoneme conversion written for TMGR. No espeak-ng code, data or output is used or distributed.\n"
    )
    open(os.path.join(out, "LICENSES.txt"), "w", encoding="utf-8").write(text)


def digest(out):
    for name in ("model.onnx", "tokens.txt", "LICENSES.txt"):
        if not os.path.exists(os.path.join(out, name)):
            continue
        data = open(os.path.join(out, name), "rb").read()
        print(f'{hashlib.sha256(data).hexdigest()}  {len(data)}  {name}')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("out")
    args = parser.parse_args()
    os.makedirs(args.out, exist_ok=True)
    work = os.path.join(args.out, ".work")
    os.makedirs(work, exist_ok=True)
    for name in ("ru_RU-denis-medium.onnx", "ru_RU-denis-medium.onnx.json"):
        if not os.path.exists(os.path.join(work, name)):
            fetch(name, work)
    prepare_model(work, args.out)
    licenses(args.out)
    digest(args.out)


if __name__ == "__main__":
    main()
