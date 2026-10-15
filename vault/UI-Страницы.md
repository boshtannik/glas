# UI: страницы и навигация

> Состояние: сверено с кодом 2026-10-15. Фактические страницы. Лента бабблов,
> обложка-микрофон и VAD пока НЕ реализованы (см. [[Флоу-UI]]).

Навигация: `ApplicationWindow` (`harbour-glas.qml`), `initialPage = DialogPage`.
Дальше — обычный `pageStack.push` из разделов. Обложка сейчас простая:
иконка микрофона + статус (`bridge.status`), без кнопки-микрофона.

## 1. DialogPage.qml (главная) — реализовано базово
- `PageHeader` «Glas» + кнопка-меню в шапке → `SettingsPage`.
- Область транскрипта: два `Label` — **статус** (`bridge.status`) и **ответ**
  (`bridge.answer`). **Ленты бабблов (Bubble/CommandBubble) нет** — это задел.
- Внизу `MicButton` (крупная PTT-кнопка): `onPressed → ptt_start`,
  `onReleased → ptt_stop` (push-to-talk, не тап).
- Кнопка-динамик слева от микрофона: `tts_speak(bridge.answer)` / `tts_stop` +
  локальный `MediaPlayer` для проигрывания WAV (`bridge.tts_file`).
- Таймер 250 мс зовёт `bridge.poll()`.

## 2. SettingsPage.qml — хаб разделов
Список-навигация (не редактор настроек): **General**, **Voice models**,
**Thinking models**, **Skills**, **Memory**.
- `GeneralSettingsPage.qml` — **скелет** (заглушка: «language, TTS, context-window
  and model-unload switches are coming later»). См. [[Настройки]].

## 3. VoiceModelsPage.qml — реализовано
Каталог STT/TTS (общий с opencode): секции «Downloaded models» / «Recognition» /
«Synthesis», фильтр по языку, скачать/выбрать/удалить, прогресс (400 мс poll),
сворачиваемые секции. Команды `voice_*` (см. [[Модели-и-каталог]]).

## 4. ThinkingModelsPage.qml — реализовано
Каталог LLM-моделей из живого Hugging Face: `SearchField` (серверный поиск с
дебаунсом 350 мс), секции «Downloaded models» / «Thinking models», пагинация
(«Load more»), плашечки влезаемости (цветная точка), PullDown «Update model
catalogs». Тап по строке → карточка модели.

## 5. ModelCardPage.qml — реализовано
Детальная карточка LLM-модели: бейдж влезаемости, размер/квант/параметры,
ленивое разрешение файла (`resolving`), прогресс, кнопки Download / Use / Remove,
и **тест-генерация на устройстве** (`llm_test`, поле промпта, результат в
`bridge.answer`).

## 6. MemoryPage.qml — реализовано
Профиль предпочтений: добавление факта (key/value + «Remember»), список «Known
facts», удаление через контекстное меню «Forget». См. [[Контекст-и-память]].

## 7. SkillsPage.qml — **скелет**
Заглушка («the skill list, .gskill import and enable toggles are coming later»).
См. [[Навыки]], [[Шаблоны-команд]].

## Навигация
- Свайпы/`pageStack` между страницами (не карусель).
- PullDown — на страницах моделей.
- Обложка — пока только индикация статуса.

См. [[Флоу-UI]], [[Модели-и-каталог]].
