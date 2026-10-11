use std::cell::RefCell;

use glas_memory::{now_secs, Memory, Source};
use qmetaobject::*;

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

    /// Long-term user preferences (`~/.local/share/harbour-glas/profile.json`).
    #[allow(dead_code)]
    memory: RefCell<Memory>,

    /// Liveness check for the bridge.
    ping: qt_method!(fn ping(&self) -> QString {
        QString::from("pong")
    }),

    /// QML: send a text request (skeleton — logs only).
    request: qt_method!(fn request(&self, text: QString) {
        log::info!("bridge.request: {:?}", text.to_string());
    }),

    /// QML: cancel the current processing.
    cancel: qt_method!(fn cancel(&self) {
        log::info!("bridge.cancel");
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
