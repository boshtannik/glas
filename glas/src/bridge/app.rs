use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use glas_core::{Command, Event, TurnState};
use glas_memory::{now_secs, Memory, Source};
use qmetaobject::*;

use crate::bridge::agent::Agent;
use crate::llm::LlmStore;
use crate::settings::Settings;
use crate::voice::{self, Recorder, VoiceStore};

/// Rust ↔ QML bridge.
///
/// Holds the model stores (voice + LLM), the push-to-talk recorder, settings
/// and long-term memory. Long-running work runs on background threads and hands
/// results back over `mpsc`; the QML polls via `poll` / `*_status_json`.
#[derive(Default, QObject)]
pub struct AppBridge {
    base: qt_base_class!(trait QObject),

    /// Current status: "idle" | "recording" | "processing" | "error".
    status: qt_property!(QString; NOTIFY status_changed),
    status_changed: qt_signal!(),

    /// Live stage text shown in the status bubble ("Слушаю…", "Обдумываю…", …).
    /// Empty when nothing is happening.
    stage: qt_property!(QString; NOTIFY stage_changed),
    stage_changed: qt_signal!(),

    /// Dialogue history as JSON: `[{"role":"user"|"assistant","text":"…"}]`.
    history: qt_property!(QString; NOTIFY history_changed),
    history_changed: qt_signal!(),

    /// Last assistant answer (feed/cover).
    answer: qt_property!(QString; NOTIFY answer_changed),
    answer_changed: qt_signal!(),

    /// WAV path of the last synthesized answer; the QML `MediaPlayer` plays it.
    tts_file: qt_property!(QString; NOTIFY tts_file_changed),
    tts_file_changed: qt_signal!(),

    /// True while a synthesis runs in the background.
    tts_busy: qt_property!(bool; NOTIFY tts_busy_changed),
    tts_busy_changed: qt_signal!(),

    /// Long-term user preferences (`~/.local/share/harbour-glas/profile.json`).
    #[allow(dead_code)]
    memory: RefCell<Memory>,

    /// User settings (`~/.config/harbour-glas/settings.json`).
    #[allow(dead_code)]
    settings: Settings,

    /// Push-to-talk recorder (parecord child process).
    recorder: Recorder,

    /// Result channel of the background transcription thread.
    rx: Option<Receiver<Result<String, String>>>,

    /// Result channel of the background TTS synthesis thread.
    tts_rx: Option<Receiver<Result<String, String>>>,

    /// Set by `tts_stop` to discard an in-flight synthesis.
    tts_cancel: Arc<AtomicBool>,

    /// WAV file of the current/last recording.
    wav: PathBuf,

    /// Shared voice-model catalog, selection and downloads.
    voice: VoiceStore,

    /// Shared LLM (thinking) model catalog, selection and downloads.
    llm: LlmStore,

    /// Result channel of the background LLM generation thread.
    gen_rx: Option<Receiver<Result<String, String>>>,

    /// The dialogue worker: owns the core, fed commands, drained in `poll`.
    agent: Agent,

    /// Dialogue history backing the `history` JSON property (role, text).
    chat: RefCell<Vec<(String, String)>>,

    /// True while the on-device LLM generates in the background.
    gen_busy: qt_property!(bool; NOTIFY gen_busy_changed),
    gen_busy_changed: qt_signal!(),

    /// Liveness check for the bridge.
    ping: qt_method!(fn ping(&self) -> QString {
        QString::from("pong")
    }),

    /// QML: send a text request (skeleton — echoes the text).
    request: qt_method!(fn request(&mut self, text: QString) {
        let text = text.to_string();
        log::info!("bridge.request: {text:?}");
        self.answer = QString::from(text);
        self.answer_changed();
    }),

    /// QML: cancel the current recording/processing/playback.
    cancel: qt_method!(fn cancel(&mut self) {
        log::info!("bridge.cancel");
        let _ = self.recorder.stop();
        self.rx = None;
        self.tts_cancel.store(true, Ordering::SeqCst);
        self.tts_rx = None;
        if self.tts_busy {
            self.tts_busy = false;
            self.tts_busy_changed();
        }
        self.agent.send(Command::Cancel);
        self.set_status_rs("idle");
    }),

    /// QML: start push-to-talk recording.
    ptt_start: qt_method!(fn ptt_start(&mut self) {
        if self.recorder.is_recording() {
            return;
        }
        let wav = std::env::temp_dir().join("glas-ptt.wav");
        match self.recorder.start(&wav) {
            Ok(()) => {
                self.wav = wav;
                self.answer = QString::from("");
                self.answer_changed();
                self.set_status_rs("recording");
                self.agent.send(Command::CaptureStart);
                log::info!("bridge.ptt_start");
            }
            Err(e) => {
                self.answer = QString::from(format!("record error: {e}"));
                self.answer_changed();
                self.set_status_rs("error");
                log::warn!("bridge.ptt_start failed: {e}");
            }
        }
    }),

    /// QML: stop recording and transcribe in the background.
    ptt_stop: qt_method!(fn ptt_stop(&mut self) {
        if !self.recorder.is_recording() {
            return;
        }
        let _ = self.recorder.stop();
        self.agent.send(Command::CaptureStop);
        log::info!("bridge.ptt_stop");

        let model = match voice::resolve_model(&self.settings.stt_model) {
            Some(model) => model,
            None => {
                self.answer =
                    QString::from("No speech model installed (Settings → Voice models)");
                self.answer_changed();
                self.set_status_rs("error");
                return;
            }
        };

        let wav = std::mem::take(&mut self.wav);
        let lang = self.settings.language.clone();
        let threads = self.settings.stt_threads.max(1);
        let audio_ctx = voice::wav_audio_ctx(&wav);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let res = voice::transcribe(&model, &lang, threads, &wav, audio_ctx);
            let _ = tx.send(res);
        });
        self.rx = Some(rx);
        self.set_status_rs("processing");
    }),

    /// QML: synthesize `text` and hand the WAV path to the QML `MediaPlayer`.
    ///
    /// Synthesis runs in the background (piper is a child process); the Rust
    /// side never touches the audio device — playback is the UI's job, mirroring
    /// the reference app.
    tts_speak: qt_method!(fn tts_speak(&mut self, text: QString) {
        self.speak_text(&text.to_string());
    }),

    /// QML: stop playback and ask a running synthesis to be discarded.
    tts_stop: qt_method!(fn tts_stop(&mut self) {
        log::info!("bridge.tts_stop");
        self.tts_cancel.store(true, Ordering::SeqCst);
        self.tts_rx = None;
        if self.tts_busy {
            self.tts_busy = false;
            self.tts_busy_changed();
        }
    }),

    /// QML: poll the background transcription (call from a Timer).
    poll: qt_method!(fn poll(&mut self) {
        // LLM generation finished: surface its text (or an error).
        if let Some(rx) = self.gen_rx.as_ref() {
            match rx.try_recv() {
                Ok(res) => {
                    self.gen_rx = None;
                    if self.gen_busy {
                        self.gen_busy = false;
                        self.gen_busy_changed();
                    }
                    match res {
                        Ok(text) => {
                            log::info!("bridge.poll llm: {text:?}");
                            self.answer = QString::from(text);
                        }
                        Err(e) => {
                            log::warn!("bridge.poll llm error: {e}");
                            self.answer = QString::from(format!("llm: {e}"));
                        }
                    }
                    self.answer_changed();
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.gen_rx = None;
                    if self.gen_busy {
                        self.gen_busy = false;
                        self.gen_busy_changed();
                    }
                }
            }
        }

        // TTS synthesis finished: surface the WAV path (or an error).
        if let Some(rx) = self.tts_rx.as_ref() {
            match rx.try_recv() {
                Ok(Ok(path)) => {
                    self.tts_rx = None;
                    self.tts_busy = false;
                    self.tts_busy_changed();
                    self.tts_file = QString::from(path);
                    self.tts_file_changed();
                }
                Ok(Err(e)) => {
                    self.tts_rx = None;
                    self.tts_busy = false;
                    self.tts_busy_changed();
                    log::warn!("bridge.poll tts error: {e}");
                    if e != "cancelled" {
                        self.answer = QString::from(format!("tts: {e}"));
                        self.answer_changed();
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.tts_rx = None;
                    self.tts_busy = false;
                    self.tts_busy_changed();
                }
            }
        }

        // Transcription finished: hand the text to the dialogue core.
        if let Some(rx) = self.rx.as_ref() {
            match rx.try_recv() {
                Ok(Ok(text)) => {
                    self.rx = None;
                    if voice::is_no_speech(&text) {
                        // Nothing was said: abandon the turn instead of feeding
                        // the model a bogus transcript (ADR-0008).
                        log::info!("bridge.poll stt: no speech, abandoning the turn");
                        self.agent.send(Command::NoSpeech);
                    } else {
                        log::info!("bridge.poll stt: {text:?}");
                        self.agent.send(Command::Transcribed { text });
                        self.set_status_rs("processing");
                    }
                }
                Ok(Err(e)) => {
                    self.rx = None;
                    log::warn!("bridge.poll stt error: {e}");
                    self.answer = QString::from(format!("error: {e}"));
                    self.answer_changed();
                    self.set_status_rs("error");
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.rx = None;
                    self.set_status_rs("error");
                }
            }
        }

        // Drain the core's events (streamed one by one; a turn may emit several).
        while let Some(event) = self.agent.try_event() {
            self.apply_event(event);
        }
    }),

    /// QML: voice store snapshot (`{models, stt, tts, lang, langs, version}`).
    voice_status_json: qt_method!(fn voice_status_json(&mut self) -> QString {
        QString::from(self.voice.status_json())
    }),

    /// QML: voice store command (`voice_download`, `voice_select_stt`, ...).
    voice_command: qt_method!(fn voice_command(&mut self, cmd: QString, val: QString) {
        let cmd = cmd.to_string();
        let val = val.to_string();
        self.voice.command(&cmd, &val);
    }),

    /// QML: LLM (thinking) store snapshot (`{models, llm, lang, langs}`).
    llm_status_json: qt_method!(fn llm_status_json(&mut self) -> QString {
        QString::from(self.llm.status_json())
    }),

    /// QML: LLM store command (`llm_download`, `llm_select`, `llm_delete`, ...).
    llm_command: qt_method!(fn llm_command(&mut self, cmd: QString, val: QString) {
        let cmd = cmd.to_string();
        let val = val.to_string();
        self.llm.command(&cmd, &val);
    }),

    /// QML: one LLM model by id (ignores the search filter) for the detail card.
    llm_model_json: qt_method!(fn llm_model_json(&mut self, id: QString) -> QString {
        QString::from(self.llm.model_json_by_id(&id.to_string()))
    }),

    /// QML: whether a GGUF LLM is selected and loadable.
    llm_available: qt_method!(fn llm_available(&self) -> bool {
        crate::infer::available()
    }),

    /// QML: run the on-device LLM on `text` (background thread; poll `poll`).
    /// The model stays resident and is dropped by the idle watchdog.
    llm_generate: qt_method!(fn llm_generate(&mut self, text: QString, max_new: i32, temperature: f64) {
        if self.gen_busy {
            return;
        }
        let text = text.to_string();
        let max_new = max_new.max(1) as usize;
        log::info!(
            "bridge.llm_generate ({} chars, max_new={max_new}, temp={temperature})",
            text.chars().count()
        );
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let res = crate::infer::generate(&text, max_new, temperature);
            let _ = tx.send(res);
        });
        self.gen_rx = Some(rx);
        self.gen_busy = true;
        self.gen_busy_changed();
    }),

    /// QML: run the on-device LLM once and free it right after (test form).
    llm_test: qt_method!(fn llm_test(&mut self, text: QString, max_new: i32, temperature: f64) {
        if self.gen_busy {
            return;
        }
        let text = text.to_string();
        let max_new = max_new.max(1) as usize;
        log::info!(
            "bridge.llm_test ({} chars, max_new={max_new}, temp={temperature})",
            text.chars().count()
        );
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let res = crate::infer::generate_once(&text, max_new, temperature);
            let _ = tx.send(res);
        });
        self.gen_rx = Some(rx);
        self.gen_busy = true;
        self.gen_busy_changed();
    }),

    /// QML: the profile as JSON (`{schema, facts:{key:{value,updated,source}}}`).
    memory_facts: qt_method!(fn memory_facts(&self) -> QString {
        QString::from(self.memory.borrow().facts_json().to_string())
    }),

    /// QML: the exact line injected into the system prompt (may be empty).
    memory_prompt_line: qt_method!(fn memory_prompt_line(&self) -> QString {
        QString::from(self.memory.borrow().prompt_line())
    }),

    /// QML: store a fact. Returns an empty string on success, else the reason.
    /// The UI must ask the user for confirmation before calling this.
    memory_set: qt_method!(fn memory_set(&self, key: QString, value: QString) -> QString {
        match self
            .memory
            .borrow_mut()
            .set(&key.to_string(), &value.to_string(), Source::User, now_secs())
        {
            Ok(()) => QString::from(""),
            Err(e) => QString::from(e.to_string()),
        }
    }),

    /// QML: forget a fact. Returns an empty string on success, else the reason.
    memory_forget: qt_method!(fn memory_forget(&self, key: QString) -> QString {
        match self.memory.borrow_mut().forget(&key.to_string()) {
            Ok(true) => QString::from(""),
            Ok(false) => QString::from("not found"),
            Err(e) => QString::from(e.to_string()),
        }
    }),
}

impl AppBridge {
    /// Update the status (main thread).
    pub fn set_status_rs(&mut self, s: &str) {
        self.status = QString::from(s);
        self.status_changed();
    }

    /// Synthesize `text` in the background and hand the WAV path to QML.
    fn speak_text(&mut self, text: &str) {
        if text.trim().is_empty() || self.tts_busy {
            return;
        }
        log::info!("bridge.speak ({} chars)", text.chars().count());
        self.tts_cancel.store(false, Ordering::SeqCst);
        let cancel = self.tts_cancel.clone();
        let text = text.to_string();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let res = voice::synthesize(&text, &cancel);
            let _ = tx.send(res.map(|p| p.to_string_lossy().into_owned()));
        });
        self.tts_rx = Some(rx);
        self.tts_busy = true;
        self.tts_busy_changed();
    }

    /// Append a bubble to the dialogue history and refresh the `history` JSON.
    fn push_bubble(&mut self, role: &str, text: &str) {
        self.chat
            .borrow_mut()
            .push((role.to_string(), text.to_string()));
        let json = self.chat_json();
        self.history = QString::from(json);
        self.history_changed();
    }

    fn chat_json(&self) -> String {
        let chat = self.chat.borrow();
        let arr: Vec<serde_json::Value> = chat
            .iter()
            .map(|(role, text)| serde_json::json!({ "role": role, "text": text }))
            .collect();
        serde_json::Value::Array(arr).to_string()
    }

    /// Update the live stage bubble (`""` hides it).
    fn set_stage(&mut self, s: &str) {
        if self.stage.to_string() == s {
            return;
        }
        self.stage = QString::from(s);
        self.stage_changed();
    }

    /// Apply one core event to the bridge's UI state.
    fn apply_event(&mut self, event: Event) {
        match event {
            Event::StateChanged(state) => {
                self.set_status_rs(state_status(state));
                self.set_stage(stage_text(state));
            }
            Event::Transcribed { text } => {
                log::info!("core heard: {text:?}");
                self.push_bubble("user", &text);
            }
            Event::Reply { text } => {
                self.answer = QString::from(text.as_str());
                self.answer_changed();
                self.push_bubble("assistant", &text);
                if self.settings.tts_mode == "auto" {
                    self.speak_text(&text);
                }
            }
            Event::NeedsConfirmation { reply, .. } => {
                self.answer = QString::from(reply.as_str());
                self.answer_changed();
                self.push_bubble("assistant", &reply);
                if self.settings.tts_mode == "auto" {
                    self.speak_text(&reply);
                }
            }
            Event::ActionResult { skill_id, summary, ok } => {
                log::info!("action {skill_id} ok={ok}: {summary}");
            }
            Event::Error { message } => {
                log::warn!("core error: {message}");
            }
            Event::SessionStarted {
                session_id,
                skill_id,
            } => log::info!("session #{session_id} opened ({skill_id})"),
            Event::SessionEnded { session_id } => {
                log::info!("session #{session_id} closed")
            }
            Event::UnknownUtterance { text } => log::info!("unknown utterance: {text:?}"),
            Event::NoSuchSkill { intent } => log::warn!("no such skill: {intent}"),
            Event::SkillDisabled { id } => log::warn!("skill disabled: {id}"),
            Event::MemorySet { key, .. } => log::info!("memory set: {key}"),
            Event::MemoryForget { key } => log::info!("memory forget: {key}"),
            Event::TurnDone => {}
        }
    }
}

/// Map the core's turn state to the three UI statuses the QML understands
/// ("idle" | "recording" | "processing").
fn state_status(state: TurnState) -> &'static str {
    match state {
        TurnState::Idle | TurnState::AwaitingConfirmation => "idle",
        TurnState::Recording => "recording",
        TurnState::Transcribing | TurnState::Thinking | TurnState::Executing | TurnState::Speaking => {
            "processing"
        }
    }
}

/// Human stage text for the live status bubble (`""` for idle).
fn stage_text(state: TurnState) -> &'static str {
    match state {
        TurnState::Idle => "",
        TurnState::Recording => "Слушаю…",
        TurnState::Transcribing => "Перевожу в текст…",
        TurnState::Thinking => "Обдумываю…",
        TurnState::AwaitingConfirmation => "Жду подтверждения…",
        TurnState::Executing => "Выполняю…",
        TurnState::Speaking => "Озвучиваю ответ…",
    }
}
