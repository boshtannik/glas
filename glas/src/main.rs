use std::cell::RefCell;
use std::ffi::CStr;
use std::io::Write as _;

use qmetaobject::{qml_register_type, QObjectPinned};
use sailors::sailfishapp::QmlApp;

use glas::bridge::app::AppBridge;

/// The only QML file of the application (`/usr/share/harbour-glas/qml`).
const MAIN_QML: &str = "qml/harbour-glas.qml";
/// Application name: determines both the data directory `SailfishApp::pathTo`
/// and argv[0].
const APP_NAME: &str = "harbour-glas";

/// Logger writes both to stderr and to `~/.cache/harbour-glas/app.log`
/// (when started via booster, stdout is inherited and lost).
struct Tee(std::fs::File);

impl std::io::Write for Tee {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        self.0.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stderr().flush();
        self.0.flush()
    }
}

fn init_logging() {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    if let Some(home) = std::env::var_os("HOME") {
        let dir = std::path::PathBuf::from(home).join(".cache/harbour-glas");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("app.log");
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            builder.target(env_logger::Target::Pipe(Box::new(Tee(file))));
        }
    }
    let _ = builder.try_init();
    log::info!("--- harbour-glas starting (pid={}) ---", std::process::id());
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    // QML ←→ Rust bridge.
    qml_register_type::<AppBridge>(
        CStr::from_bytes_with_nul(b"Glas\0")?,
        1,
        0,
        CStr::from_bytes_with_nul(b"AppBridge\0")?,
    );
    let bridge = RefCell::new(AppBridge::default());
    let bridge_pinned = unsafe { QObjectPinned::new(&bridge) };

    // QML runs on the main thread; exec() blocks until the window is closed.
    let mut app = QmlApp::application(APP_NAME.to_string());
    app.set_object_property("bridge".into(), bridge_pinned);
    app.set_source(QmlApp::path_to(MAIN_QML.into()));
    app.show_full_screen();
    app.exec();

    log::info!("QML closed, exiting");
    Ok(())
}
