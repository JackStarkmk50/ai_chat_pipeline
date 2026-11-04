# ai_chat_pipeline (Reply Voice)

A small background app for Windows and Linux that reads replies aloud. It listens on this
computer only, and speaks the reply text it is given. Nothing else is read, stored or sent.

Status: early build. English only.

## How it is built

- **Tauri 2**: the screens are a web frontend (React + TypeScript + Tailwind, built with
  Vite) in the system webview; everything underneath is Rust.
- **`core/`** (`reply_voice` crate): the working parts with no UI - config, the local
  receiver, the speech queue, the voice engines, the download manager, autostart. Its
  tests run without a window.
- **`src-tauri/`**: the thin shell that wires the core to the window, tray and commands.

## What works and what is tested

Tested on Linux, in CI (core tests run on Linux and Windows runners; the full app builds
on both):

- Local receiver: `POST /speak` with `Authorization: Bearer <token>` and `{"text": "..."}`.
  Wrong or missing token gets 401. Empty, too long (over 8000 bytes) or malformed bodies
  are rejected. Replies are read one at a time, in order.
- Listens on loopback addresses only. The receiver stays off until a token is set.
- Settings file, speed limits (0.75x to 1.5x), token and address checks.
- Start with the computer is off by default. The Linux autostart file is written and
  removed correctly.
- Reply text goes to the voice program on stdin, never on the command line.
- Download and checksum handling: a good set installs, a wrong checksum is thrown away,
  a failed download is reported.
- The screens (first run, home, settings) are checked as rendered pages in light and
  dark against the approved Cue design.

Written but not run on a real Windows machine (built in CI, not exercised on a desktop):

- Voice through the built-in Windows speech (PowerShell, System.Speech).
- Tray icon, and closing the window to keep running in the background.
- Start with the computer through the Windows Run registry key.

Voice engine (no model is chosen or shipped):

- Voices run through the Piper program (MIT licensed). No voice model, and no default
  download, is built in. The model to use is decided separately. To enable the
  "Download voice" button, put a `voice-manifest.json` next to the settings file (the
  app's config folder). Nothing is downloaded until the button is pressed. Every file is
  checked against the checksum in the manifest, and a file with the wrong checksum is
  thrown away. Only https links are allowed. The model's licence and redistribution
  terms need to be checked by whoever picks it.
- Manifest shape (fill in a Piper release archive for your platform and a model you have
  checked the licence of):

  {"name": "Voice name shown in Settings", "model_file": "model.onnx", "items": [
    {"url": "https://.../piper_archive", "sha256": "<64 hex>", "size": 123, "file": "piper.tgz", "extract": true},
    {"url": "https://.../model.onnx.json", "sha256": "<64 hex>", "size": 123, "file": "model.onnx.json", "extract": false},
    {"url": "https://.../model.onnx", "sha256": "<64 hex>", "size": 123, "file": "model.onnx", "extract": false}]}

- With no manifest, or before the download, the computer's own voice (`espeak-ng` on
  Linux, Windows speech) is used if it exists and is labelled "System voice (fallback)".
  If there is neither, the app says nothing can be read aloud yet.
- Download, checksum and failure handling are tested with local fixture files. A real
  Piper download and synthesis was run once on Linux (previous eframe build, same core
  code): the audio file was non-silent. Nobody listened to it, and real speaker output
  is unverified on every platform.
- Playing the sound uses `paplay`, `aplay`, `ffplay` or `play` on Linux and
  System.Media.SoundPlayer on Windows.

Not done:

- Nothing sends replies to this app yet. The status only says "Waiting for a reply",
  then "Reply received" after a reply with the right token arrives. A local test post
  does not show the app is connected to Instinct. The token is stored in the settings
  file in plain text (owner-only permissions on Linux).
- The Tauri shell is built and tested in CI; it has not been run on a real desktop yet.
- No tests ran on macOS.

## Develop

```
npm install
npm run dev        # frontend only, with a demo state per URL hash (browser)
npm run tauri dev  # the real app
```

Core tests need no window: `cargo test -p reply_voice`.

Send a test reply:

```
curl -X POST http://127.0.0.1:47615/speak \
  -H 'Authorization: Bearer YOUR_TOKEN' \
  -d '{"text":"Hello"}'
```
