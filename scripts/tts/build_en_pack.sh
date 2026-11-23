#!/usr/bin/env bash
set -euo pipefail

OUT="${1:?usage: build_en_pack.sh <output-dir>}"
VOICE_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-en_US-ljspeech-medium-int8.tar.bz2"
VOICE_SHA="${VOICE_SHA:-}"
CMUDICT_COMMIT="74790861f652b15e4ac49015a90074ad62a27690"
CMUDICT_URL="https://raw.githubusercontent.com/cmusphinx/cmudict/${CMUDICT_COMMIT}/cmudict.dict"
CMUDICT_LICENSE_URL="https://raw.githubusercontent.com/cmusphinx/cmudict/${CMUDICT_COMMIT}/LICENSE"
ALLOWED="model.onnx tokens.txt cmudict.dict LICENSES.txt"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$OUT"

curl -fsSL -o "$work/voice.tar.bz2" "$VOICE_URL"
if [ -n "$VOICE_SHA" ]; then
  echo "$VOICE_SHA  $work/voice.tar.bz2" | shasum -a 256 -c -
fi
mkdir "$work/voice"
tar xjf "$work/voice.tar.bz2" -C "$work/voice" \
  --include='*/en_US-ljspeech-medium.onnx' --include='*/tokens.txt'
dir="$(find "$work/voice" -mindepth 1 -maxdepth 1 -type d | head -n 1)"
cp "$dir/en_US-ljspeech-medium.onnx" "$OUT/model.onnx"
cp "$dir/tokens.txt" "$OUT/tokens.txt"
curl -fsSL -o "$OUT/cmudict.dict" "$CMUDICT_URL"
curl -fsSL -o "$work/cmudict.license" "$CMUDICT_LICENSE_URL"

{
  echo "Voice: en_US ljspeech (medium), int8 build from k2-fsa/sherpa-onnx."
  echo "Dataset: LJ Speech (https://keithito.com/LJ-Speech-Dataset/), public domain."
  echo "Model card: rhasspy/piper-voices en/en_US/ljspeech/medium, license: public domain."
  echo "Quantisation and tokens.txt: k2-fsa/sherpa-onnx, Apache-2.0."
  echo
  echo "Pronunciation dictionary: CMU Pronouncing Dictionary, commit ${CMUDICT_COMMIT}."
  echo
  cat "$work/cmudict.license"
} >"$OUT/LICENSES.txt"

for f in "$OUT"/*; do
  name="$(basename "$f")"
  case " $ALLOWED " in *" $name "*) ;; *) echo "unexpected file in pack: $name" >&2; exit 1 ;; esac
done
cd "$OUT"
for f in $ALLOWED; do
  printf '%s  %s  %s\n' "$(shasum -a 256 "$f" | cut -d' ' -f1)" "$(wc -c <"$f" | tr -d ' ')" "$f"
done
