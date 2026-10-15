# Glas

On-device voice AI assistant for Sailfish OS (Rust + QML). It is intended to
control phone functions (timer, alarm, reading/sending SMS) through "skills" and
works fully offline: STT (whisper.cpp), LLM (candle, GGUF), TTS (piper).

## Status

Work in progress. The Rust↔QML bridge, the SDK/RPM build and the on-device
inference path are working:

- **STT** — push-to-talk (`parecord` → `whisper-cli`, bundled with the
  `--audio-ctx` patch).
- **TTS** — `piper` (bundled), with an on-disk WAV cache; playback in QML.
- **LLM** — `candle` (pure Rust, no bundled C runtime); GGUF loaded on-device,
  dispatched by `general.architecture` (llama/qwen2/qwen3/gemma/phi2/phi3/glm4/lfm2),
  dropped by an idle watchdog.
- **Model catalogs** — a live Hugging Face catalog for LLMs, and the STT/TTS
  catalog shared with harbour-opencode.
- **Long-term memory** — `glas-memory` (`profile.json`), edited on the Memory page.

Still ahead: intent extraction (`{intent, params, reply}`), the `skills` engine,
history/undo, the cover microphone + VAD, and the bubble feed. See `vault/План.md`.

## Layout

```
glas/            Cargo package (bin harbour-glas, lib glas)
  src/           main.rs, lib.rs, bridge/, cmd.rs, settings.rs, voice.rs, infer.rs, llm.rs, audio/
  memory/        glas-memory crate (long-term preferences)
  qml/           harbour-glas.qml + pages/ + components/
  vendor/        qmetaobject-sfos-rs + sailors (path dependencies)
  assets/        models.json (packaged voice catalog — currently empty)
rpm/             harbour-glas.spec
packaging/       engine patches (whisper-cli --audio-ctx)
scripts/         build.sh / deploy.sh
mbuild.sh        build the RPM via mb2 in Docker
vault/           project documentation (Obsidian)
```

## Building

The RPM is built on a PC in the sailo-rs Docker image (the phone has no cargo,
and its root partition is almost full):

```sh
sg docker -c "./mbuild.sh"            # debug
sg docker -c "./mbuild.sh --release"  # release
```

Host check/build (requires the cross toolchain, see `.cargo/config.toml`):

```sh
scripts/build.sh        # debug, host
scripts/build.sh -ar    # release, aarch64
```

## Settings

`~/.config/harbour-glas/settings.json` (language, context window, TTS mode,
model unload timeout).

## License

MIT. See `LICENSE`.
