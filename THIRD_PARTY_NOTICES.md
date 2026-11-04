# Third-party notices

Reply Voice builds on these projects. Thank you to their authors.

## Dictation pipeline (adopted design)

The microphone -> voice-activity -> transcription pipeline adopts its design
and its worker-process wire protocol from **Handy**
(https://github.com/cjpais/Handy), Copyright (c) 2025 CJ Pais, MIT License.
Process supervision, the framed `[json][pcm]` protocol, the stdout-claim
trick, and the GPU-fault CPU retry follow Handy's engine_supervisor.

## Crates

- **handy-recorder** 0.1 (https://github.com/cjpais/handy-recorder) - MIT.
  Cross-platform microphone capture; includes an adapted Silero VAD
  (https://github.com/snakers4/silero-vad), MIT.
- **earshot** 1.2 (https://github.com/cjpais/earshot) - MIT OR Apache-2.0.
  Silero VAD inference in Rust.
- **transcribe-cpp** 0.4.0 + **transcribe-cpp-sys** 0.4.0
  (https://github.com/handy-computer/transcribe.cpp) - MIT. GGUF/ggml
  transcription engine (whisper.cpp/ggml lineage). Kept in lockstep with
  Handy's catalog (schema 2, transcribeCpp 0.4.0).

## Models (downloaded by the user, checked against stt-manifest.json)

- **Parakeet Unified English 0.6B** (GGUF by handy-computer, based on NVIDIA
  parakeet-unified-en-0.6b) - the upstream model is licensed under the
  **NVIDIA Open Model License Agreement**
  (https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-license/);
  the GGUF repository's metadata marks it CC-BY-4.0 and its card links the
  NVIDIA terms. (Handy's catalog tags this family cc-by-4.0, following the
  GGUF repo metadata.)

  Required attribution notice: "Licensed by NVIDIA Corporation under the
  NVIDIA Open Model License". The Agreement is use-acceptance based: by
  using the model the user accepts its terms, which include NVIDIA's
  Trustworthy AI terms
  (https://www.nvidia.com/en-us/agreements/trustworthy-ai/terms/), a
  litigation-termination clause, guardrail-preservation, indemnity, and
  trade-compliance conditions. The model is commercially usable and outputs
  are not claimed by NVIDIA. This app does not redistribute the model: it
  downloads it from the public handy-computer Hugging Face repository on the
  user's action, with the SHA-256 pinned in stt-manifest.json.
- **Whisper Large v3 Turbo** and **Whisper Small** (GGUF by handy-computer,
  based on OpenAI Whisper) - MIT per the model repositories
  (handy-computer/whisper-*-gguf). Handy's catalog tags these "apache-2.0";
  the pinned Hugging Face repositories and OpenAI's Whisper are MIT. Both
  licences are permissive; neither requires attribution.

Model metadata (repository, pinned revision, file size, SHA-256) is lifted
from Handy's catalog without re-deriving it.
