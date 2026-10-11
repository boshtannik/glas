# Glas

Локальный голосовой ИИ-ассистент для Sailfish OS (Rust + QML). Управляет
функциями телефона (таймер, будильник, чтение/отправка SMS) через «навыки»,
работает полностью офлайн: STT (whisper.cpp), LLM (llama.cpp, GGUF + GBNF),
TTS (piper).

## Статус

Стоп-точка №1 — **каркас**: пустое окно Silica + мост Rust ↔ QML.
Аудио-модули (`src/audio/`) скопированы из референса и собираются только с
фичей `voice`. Распознавание, LLM-интент, навыки и каталог моделей — впереди
(см. `vault/План.md`).

## Структура

```
glas/            Cargo-пакет (bin harbour-glas, lib glas)
  src/           main.rs, lib.rs, bridge/, cmd.rs, settings.rs, audio/
  qml/           harbour-glas.qml + pages/ + components/
  vendor/        qmetaobject-sfos-rs + sailors (path-зависимости)
  assets/        models.json (каталог голосовых моделей)
rpm/             harbour-glas.spec
packaging/       патчи движков (whisper-cli --audio-ctx)
scripts/         build.sh / deploy.sh
mbuild.sh        сборка RPM через mb2 в Docker
vault/           проектная документация (Obsidian)
```

## Сборка

Сборка RPM выполняется на ПК в Docker-образе sailo-rs (на телефоне нет cargo,
а корневой раздел почти полон):

```sh
sg docker -c "./mbuild.sh"            # debug
sg docker -c "./mbuild.sh --release"  # release
```

Проверка/хост-сборка (нужен крестовый toolchain, см. `.cargo/config.toml`):

```sh
scripts/build.sh        # debug, host
scripts/build.sh -ar    # release, aarch64
```

## Настройки

`~/.config/harbour-glas/settings.json` (язык, окно контекста, режим TTS,
таймаут выгрузки моделей).

## Лицензия

MIT. См. `LICENSE`.
