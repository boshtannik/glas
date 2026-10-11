use qmetaobject::*;

/// Rust ↔ QML bridge.
///
/// Skeleton: a status property and a couple of methods. The real version will
/// add `Arc<Mutex<..>>` queues and a worker (see the mason reference,
/// `bridge/app.rs`).
#[derive(Default, QObject)]
pub struct AppBridge {
    base: qt_base_class!(trait QObject),

    /// Current status: "idle" | "recording" | "processing" | "error".
    status: qt_property!(QString; NOTIFY status_changed),
    status_changed: qt_signal!(),

    /// Last assistant answer (feed/cover).
    answer: qt_property!(QString; NOTIFY answer_changed),
    answer_changed: qt_signal!(),

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
}

impl AppBridge {
    /// Update the status (main thread).
    pub fn set_status_rs(&mut self, s: &str) {
        self.status = QString::from(s);
        self.status_changed();
    }
}
