use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use glas_memory::{now_secs, Memory, Source};
use qmetaobject::*;

use crate::settings::Settings;
use crate::voice::{self, Recorder, VoiceStore};

/// Rust ↔ QML bridge.
///
/// Skeleton: a status property, a couple of methods and the long-term memory
/// store. The real version will add `Arc<Mutex<..>>` queues and a worker
/// (see the mason reference, `bridge/app.rs`).
#[derive(Default, QObject)]
pub struct AppBridge {
    base: qt_base_class!(trait QObject),

    /// Current status: "idle" | "recording" | "processing" | "error".
    status: qt_property!(QString; NOTIFY status_changed),
    status_changed: qt_signal!(),

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
        let text = text.to_string();
        if text.trim().is_empty() || self.tts_busy {
            return;
        }
        log::info!("bridge.tts_speak ({} chars)", text.chars().count());
        self.tts_cancel.store(false, Ordering::SeqCst);
        let cancel = self.tts_cancel.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let res = voice::synthesize(&text, &cancel);
            let _ = tx.send(res.map(|p| p.to_string_lossy().into_owned()));
        });
        self.tts_rx = Some(rx);
        self.tts_busy = true;
        self.tts_busy_changed();
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

        let msg = match self.rx.as_ref() {
            Some(rx) => match rx.try_recv() {
                Ok(res) => Some(res),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("worker stopped".to_string()))
                }
            },
            None => None,
        };
        let res = match msg {
            Some(res) => res,
            None => return,
        };
        self.rx = None;
        match res {
            Ok(text) => {
                let text = if text.is_empty() {
                    "(no speech)".to_string()
                } else {
                    text
                };
                log::info!("bridge.poll: {text:?}");
                self.answer = QString::from(text);
            }
            Err(e) => {
                log::warn!("bridge.poll error: {e}");
                self.answer = QString::from(format!("error: {e}"));
            }
        }
        self.answer_changed();
        self.set_status_rs("idle");
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
}
