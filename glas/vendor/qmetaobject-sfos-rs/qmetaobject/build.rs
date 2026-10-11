/* Copyright (C) 2018 Olivier Goffart <ogoffart@woboq.com>

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
associated documentation files (the "Software"), to deal in the Software without restriction,
including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense,
and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial
portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES
OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
*/
extern crate cpp_build;
use semver::Version;
use std::process::Command;
use std::path::Path;
use std::io::prelude::*;
use std::io::BufReader;

fn qmake_query(var: &str) -> String {
    let qmake = std::env::var("QMAKE").unwrap_or("qmake".to_string());
    String::from_utf8(
        Command::new(qmake)
            .env("QT_SELECT", "qt5")
            .args(&["-query", var])
            .output()
            .expect("Failed to execute qmake. Make sure 'qmake' is in your path")
            .stdout,
    )
        .expect("UTF-8 conversion failed")
}

// qreal is a double, unless QT_COORD_TYPE says otherwise:
// https://doc.qt.io/qt-5/qtglobal.html#qreal-typedef
fn detect_qreal_size(qt_include_path: &str) {
    let path = Path::new(qt_include_path).join("QtCore").join("qconfig.h");
    let f = std::fs::File::open(&path).expect(&format!("Cannot open `{:?}`", path));
    let b = BufReader::new(f);

    // Find declaration of QT_COORD_TYPE
    for line in b.lines() {
        let line = line.expect("qconfig.h is valid UTF-8");
        if line.contains("QT_COORD_TYPE") {
            if line.contains("float") {
                println!("cargo:rustc-cfg=qreal_is_float");
                return;
            } else {
                panic!("QT_COORD_TYPE with unknown declaration {}", line);
            }
        }
    }

}

fn main() {
    let mer_sdk = std::env::var("MERSDK").ok();
    let mer_target = std::env::var("MER_TARGET").ok().unwrap_or("SailfishOS-latest".into());

    let mer = match mer_sdk {
        Some(a) => {
            let arch = match &std::env::var("CARGO_CFG_TARGET_ARCH").unwrap() as &str {
                "arm" => "armv7hl",
                "aarch64" => "aarch64",
                "i686" => "i486",
                "x86" => "i486",
                unsupported => panic!("Target {} is not supported for Mer", unsupported),
            };
            Some((a, mer_target, arch))
        },
        _ => None,
    };

    let mut config = cpp_build::Config::new();

    let macos_lib_search = if cfg!(target_os = "macos") {
        "=framework"
    } else {
        ""
    };

    match mer {
        Some((sdk, target, arch)) => {
            config.flag(&format!("--sysroot=/{}/targets/{}-{}", sdk, target, arch));
            config.flag("-isysroot");
            config.flag(&format!("/{}/targets/{}-{}", sdk, target, arch));

            config.include(format!("/{}/targets/{}-{}/usr/include/", sdk, target, arch));
            let qt_include_path = format!("/{}/targets/{}-{}/usr/include/qt5/", sdk, target, arch);
            config.include(&qt_include_path);

            detect_qreal_size(&qt_include_path);
        },
        None => {
            let qt_include_path = qmake_query("QT_INSTALL_HEADERS");
            let qt_library_path = qmake_query("QT_INSTALL_LIBS");
            let qt_version = qmake_query("QT_VERSION")
                .parse::<Version>()
                .expect("Parsing Qt version failed");

            config.include(qt_include_path.trim());
            if qt_version >= Version::new(5, 14, 0) {
                println!("cargo:rustc-cfg=qt_5_14");
            }

            detect_qreal_size(&qt_include_path.trim());

            if cfg!(target_os = "macos") {
                config.flag("-F");
            }

            println!(
                "cargo:rustc-link-search{}={}",
                macos_lib_search,
                qt_library_path.trim()
            );
        },
    }

    config.build("src/lib.rs");

    let macos_lib_framework = if cfg!(target_os = "macos") { "" } else { "5" };
    println!(
        "cargo:rustc-link-lib{}=Qt{}Widgets",
        macos_lib_search, macos_lib_framework
    );
    println!(
        "cargo:rustc-link-lib{}=Qt{}Gui",
        macos_lib_search, macos_lib_framework
    );
    println!(
        "cargo:rustc-link-lib{}=Qt{}Core",
        macos_lib_search, macos_lib_framework
    );
    println!(
        "cargo:rustc-link-lib{}=Qt{}Quick",
        macos_lib_search, macos_lib_framework
    );
    println!(
        "cargo:rustc-link-lib{}=Qt{}Qml",
        macos_lib_search, macos_lib_framework
    );
}
