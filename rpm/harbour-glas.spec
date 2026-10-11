Name:       harbour-glas
Summary:    Glas — a voice assistant that controls the phone
Version:    0.1
Release:    1
Group:      Qt/Qt
License:    MIT
URL:        https://github.com/boshtannik/glas
Source0:    %{name}-%{version}.tar.bz2
Requires:   sailfishsilica-qt5 >= 0.10.9
Requires:   libsailfishapp
BuildRequires:  rust
BuildRequires:  cargo
BuildRequires:  pkgconfig(Qt5Core)
BuildRequires:  pkgconfig(Qt5Qml)
BuildRequires:  pkgconfig(Qt5Quick)
BuildRequires:  pkgconfig(sailfishapp)

# Do not let rpmbuild strip the bundle (same as the reference): brp-strip can
# corrupt the nested engine binaries.
%global debug_package %{nil}
%global __strip /bin/true

%description
Glas — an on-device voice assistant for Sailfish OS: speech recognition,
a local LLM, speech synthesis and skills that drive phone functions (timer,
alarm, SMS). Works offline.

# - PREP -----------------------------------------------------------------------
%prep
%setup -q -n %{name}-%{version}

# - BUILD ----------------------------------------------------------------------
%build
cd glas
MODE="${MBUILD_MODE:-debug}"
echo "harbour-glas: MBUILD_MODE=$MODE"
export QMAKE=/usr/lib64/qt5/bin/qmake
export SB2_RUST_TARGET_TRIPLE=aarch64-unknown-linux-gnu
export CC_aarch64_unknown_linux_gnu=aarch64-meego-linux-gnu-gcc
export CXX_aarch64_unknown_linux_gnu=aarch64-meego-linux-gnu-g++
export AR_aarch64_unknown_linux_gnu=aarch64-meego-linux-gnu-ar
case "$MODE" in
  release)
    export RUSTFLAGS="-Clink-arg=-Wl,-z,relro,-z,now -Ccodegen-units=1 -Clink-arg=-rdynamic"
    export CARGO_INCREMENTAL=0
    export CARGO_TARGET_DIR=/home/mersdk/cargo-cache/cargo/release
    cargo build --release -j 1
    ;;
  *)
    export RUSTFLAGS="-Clink-arg=-Wl,-z,relro,-z,now -Clink-arg=-rdynamic"
    export CARGO_INCREMENTAL=1
    export CARGO_TARGET_DIR=/home/mersdk/cargo-cache/cargo/debug
    cargo build -j 1
    ;;
esac

# whisper.cpp: a static whisper-cli (STT). Prepared by mbuild.sh (symlink).
cd ../third_party/whisper.cpp
make -j1 \
  UNAME_M=generic \
  CC=aarch64-meego-linux-gnu-gcc CXX=aarch64-meego-linux-gnu-g++ \
  AR=aarch64-meego-linux-gnu-ar \
  main || true
test -x main
ls -l main

# Piper (TTS): a ready-made aarch64 build from the cache (downloaded by mbuild.sh).
test -x /home/mersdk/cargo-cache/piper-aarch64/piper/piper
ls -l /home/mersdk/cargo-cache/piper-aarch64/piper

# TODO(LLM stage): build llama.cpp (libllama + server/CLI) for the intent engine.

# - INSTALL --------------------------------------------------------------------
%install
MODE="${MBUILD_MODE:-debug}"
rm -rf %{buildroot}
install -Dm 755 /home/mersdk/cargo-cache/cargo/$MODE/aarch64-unknown-linux-gnu/$MODE/harbour-glas -t %{buildroot}%{_bindir}
install -Dm 755 third_party/whisper.cpp/main %{buildroot}%{_libexecdir}/%{name}/whisper-cli
mkdir -p %{buildroot}%{_libexecdir}/%{name}/piper
cp -a /home/mersdk/cargo-cache/piper-aarch64/piper/. %{buildroot}%{_libexecdir}/%{name}/piper/
install -Dm 644 glas/assets/models.json -t %{buildroot}%{_datadir}/%{name}
install -Dm 644 harbour-glas.png -t %{buildroot}%{_datadir}/icons/hicolor/86x86/apps
install -Dm 644 harbour-glas.desktop -t %{buildroot}%{_datadir}/applications
install -d %{buildroot}%{_datadir}/%{name}
cp -r glas/qml %{buildroot}%{_datadir}/%{name}/qml

desktop-file-install --delete-original    \
  --dir %{buildroot}%{_datadir}/applications    \
   %{buildroot}%{_datadir}/applications/*.desktop

# - CHECK ----------------------------------------------------------------------
%check
MODE="${MBUILD_MODE:-debug}"
if nm -D /home/mersdk/cargo-cache/cargo/$MODE/aarch64-unknown-linux-gnu/$MODE/harbour-glas | grep " T main$" > /dev/null ; then
  echo "main symbol exists"
else
  echo "main symbol is missing"
  exit 1
fi

# - FILES ----------------------------------------------------------------------
%files
%defattr(-,root,root,-)
%{_bindir}/harbour-glas
%{_libexecdir}/%{name}/whisper-cli
%{_libexecdir}/%{name}/piper
%{_datadir}/%{name}/qml
%{_datadir}/%{name}/models.json
%{_datadir}/applications/%{name}.desktop
%{_datadir}/icons/hicolor/86x86/apps/%{name}.png
