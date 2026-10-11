# Glas

On-device voice AI assistant for Sailfish OS (Rust + QML). It controls phone
functions (timer, alarm, reading/sending SMS) through "skills" and works fully
offline: STT (whisper.cpp), LLM (llama.cpp, GGUF + GBNF), TTS (piper).

## Status

Stop-point #1 — **skeleton**: an empty Silica window plus the Rust ↔ QML bridge.
The audio modules (`src/audio/`) are copied from the reference and are built only
with the `voice` feature. Speech recognition, LLM intent, skills and the model
catalog are still ahead (see `vault/План.md`).

## Layout

```
glas/            Cargo package (bin harbour-glas, lib glas)
  src/           main.rs, lib.rs, bridge/, cmd.rs, settings.rs, audio/
  qml/           harbour-glas.qml + pages/ + components/
  vendor/        qmetaobject-sfos-rs + sailors (path dependencies)
  assets/        models.json (voice model catalog)
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
