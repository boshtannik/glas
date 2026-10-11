use qmetaobject::*;

/// Мост Rust ↔ QML.
///
/// Каркас: статус и пара методов. В рабочей версии здесь будут очереди
/// `Arc<Mutex<..>>` и воркер (см. референс mason, `bridge/app.rs`).
#[derive(Default, QObject)]
pub struct AppBridge {
    base: qt_base_class!(trait QObject),

    /// Текущий статус: "idle" | "recording" | "processing" | "error".
    status: qt_property!(QString; NOTIFY status_changed),
    status_changed: qt_signal!(),

    /// Последний ответ ассистента (лента/обложка).
    answer: qt_property!(QString; NOTIFY answer_changed),
    answer_changed: qt_signal!(),

    /// Проверка живости моста.
    ping: qt_method!(fn ping(&self) -> QString {
        QString::from("pong")
    }),

    /// QML: отправить текстовый запрос (каркас — только лог).
    request: qt_method!(fn request(&self, text: QString) {
        log::info!("bridge.request: {:?}", text.to_string());
    }),

    /// QML: отменить текущую обработку.
    cancel: qt_method!(fn cancel(&self) {
        log::info!("bridge.cancel");
    }),
}

impl AppBridge {
    /// Обновить статус (main thread).
    pub fn set_status_rs(&mut self, s: &str) {
        self.status = QString::from(s);
        self.status_changed();
    }
}
