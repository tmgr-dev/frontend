# External voice for the Manager window

The Manager window can read agent replies aloud with a voice program that runs on your own machine. This is the "Voice" section of the Manager window. TMGR does not ship or download any third-party voice model for this; you install the voice yourself and you are responsible for its licence. The Silero example below is only an example of such a voice.

Order used for every message: external voice (if configured and healthy), then the built-in Piper voice for the message language (if downloaded), then the system voice. If the external voice fails, the error stays visible in the Voice section and the next message uses the next voice until you run "Test voice" successfully again or save the settings (saving also resets the state). "Test voice" uses the saved settings and a phrase you can edit; use a phrase in a language your voice supports, because a Russian-only voice cannot read English text.

## Two ways to connect

### 1. OpenAI-compatible HTTP server

Set the mode to "OpenAI-compatible server" and fill in the base URL, for example `http://127.0.0.1:8000`. TMGR sends `POST <base>/v1/audio/speech` with JSON `{"model": "...", "input": "...", "voice": "...", "response_format": "wav"}` and expects a WAV file in the response. Model and voice are optional.

Only loopback addresses (`localhost`, `127.x.x.x`, `::1`) are accepted by default. The text of your chats is sent to this server, so a non-local server needs the explicit "Allow non-local server" switch and must use `https`, and TMGR asks you to confirm the exact address when you turn the switch on and whenever the host of a non-local server changes. There is no field for an API key; servers that require an `Authorization` header cannot be used directly. Redirects and system proxies are never followed.

### 2. Local command

Set the mode to "Local command" and enter the program as separate arguments (not as one shell string). The first argument must be an absolute path. TMGR starts the program without a shell, writes the text to its standard input as UTF-8, closes it, and reads a WAV file from its standard output. There is a time limit (default 30 seconds) and a size limit (30 MB). On failure the first 2 KB of the standard error are shown. TMGR asks you to confirm the exact command line whenever it changes. Do not put secrets in the arguments: the settings are stored as plain JSON in the app data folder.

## Example: Silero running locally

Silero models are published by Silero Team under their own licence (CC BY-NC-SA 4.0 for most voices, which excludes commercial use). You install them and you decide whether your use is allowed. Nothing Silero-related is part of TMGR.

Install the Python packages in a virtual environment you create for this (see the Silero README for current versions):

```bash
python3 -m venv ~/silero-venv
~/silero-venv/bin/pip install torch
```

Save this as `silero_server.py`. It loads the model through the public `torch.hub` API on first start, which downloads the model into the torch cache on your machine, and serves the OpenAI-style route on loopback:

```python
import io
import json
import os
import wave
from http.server import BaseHTTPRequestHandler, HTTPServer

import torch

LANGUAGE = os.environ["VOICE_LANGUAGE"]
MODEL_ID = os.environ["VOICE_MODEL"]
SPEAKER = os.environ["VOICE_SPEAKER"]
SAMPLE_RATE = 48000

model, _ = torch.hub.load(
    repo_or_dir="snakers4/silero-models",
    model="silero_tts",
    language=LANGUAGE,
    speaker=MODEL_ID,
)
model.to(torch.device("cpu"))


def to_wav(samples):
    pcm = (samples.clamp(-1, 1) * 32767).short().numpy().tobytes()
    buffer = io.BytesIO()
    with wave.open(buffer, "wb") as out:
        out.setnchannels(1)
        out.setsampwidth(2)
        out.setframerate(SAMPLE_RATE)
        out.writeframes(pcm)
    return buffer.getvalue()


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path != "/v1/audio/speech":
            self.send_error(404)
            return
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        speaker = body.get("voice") or SPEAKER
        audio = model.apply_tts(text=body["input"], speaker=speaker, sample_rate=SAMPLE_RATE)
        data = to_wav(audio)
        self.send_response(200)
        self.send_header("Content-Type", "audio/wav")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


HTTPServer(("127.0.0.1", 8000), Handler).serve_forever()
```

Start it with the language, model id and speaker you chose from the Silero README, for example `VOICE_LANGUAGE=ru VOICE_MODEL=<model id> VOICE_SPEAKER=<speaker> ~/silero-venv/bin/python silero_server.py`. In the Voice section choose "OpenAI-compatible server", base URL `http://127.0.0.1:8000`, and press "Test voice".

If you prefer not to keep a server running, the same model can be used as a local command: a script that reads all of standard input as text, calls `model.apply_tts`, and writes the WAV bytes to standard output. Enter the full path of the virtual environment's `python` (a leading `~` is not expanded, so write the whole path) and the script path as two arguments.

## Other engines

Anything that exposes the same route (many local TTS servers do) or can turn text on standard input into WAV on standard output works, for example the Piper command line or a wrapper around the system `say` command followed by `afconvert`.
