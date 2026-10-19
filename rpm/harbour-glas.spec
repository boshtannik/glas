Name:       harbour-glas
Summary:    Glas — a voice assistant that controls the phone
Version:    0.1.0
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

# Heavy *helper* payload (whisper-cli, piper) is installed under %{payloaddir}
# on the /home partition — a large (tens of GB) separate filesystem — and only
# the /usr/libexec/%{name} symlink lives in /usr (on the small, often-full root
# partition). The app addresses its helpers by absolute /usr/libexec paths,
# which resolve through the symlink; the desktop file disables sailjail
# sandboxing, so /home is reachable at runtime.
#
# NOTE: the GUI launcher (harbour-glas) MUST stay a real file in %{_bindir}.
# libsailfishapp derives the app data dir from the *executable's directory*
# (SailfishAppPriv::dataDir replaces the last path component of the exe's dir
# with "share/"). A symlinked launcher resolves to /home/%{name}, so the app
# would look for its QML under /home/share/%{name} → white screen.
%define payloaddir /home/%{name}

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
    export RUSTFLAGS="-Clink-arg=-Wl,-z,relro,-z,now -Clink-arg=-rdynamic -Copt-level=1"
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

# - INSTALL --------------------------------------------------------------------
%install
MODE="${MBUILD_MODE:-debug}"
rm -rf %{buildroot}
# GUI launcher → real file in %{_bindir} (see the NOTE above: its location
# determines the app data dir). Heavy helpers → %{payloaddir} (on /home).
install -Dm 755 /home/mersdk/cargo-cache/cargo/$MODE/aarch64-unknown-linux-gnu/$MODE/harbour-glas -t %{buildroot}%{_bindir}
PAYLOAD=%{buildroot}%{payloaddir}
install -Dm 755 third_party/whisper.cpp/main $PAYLOAD/whisper-cli
mkdir -p $PAYLOAD/piper
cp -a /home/mersdk/cargo-cache/piper-aarch64/piper/. $PAYLOAD/piper/
install -Dm 644 glas/assets/models.json -t %{buildroot}%{_datadir}/%{name}
# /usr/libexec/%{name} is a symlink to %{payloaddir}; the app resolves its
# helpers through it.
install -d %{buildroot}%{_libexecdir}
ln -s %{payloaddir} %{buildroot}%{_libexecdir}/%{name}
# Launcher icons in all recommended sizes (Sailfish icon guidelines).
for size in 86x86 108x108 128x128 172x172; do
  install -Dm 644 icons/$size/harbour-glas.png \
    %{buildroot}%{_datadir}/icons/hicolor/$size/apps/harbour-glas.png
done
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
%{payloaddir}
%{_bindir}/harbour-glas
%{_libexecdir}/%{name}
%{_datadir}/%{name}/qml
%{_datadir}/%{name}/models.json
%{_datadir}/applications/%{name}.desktop
%{_datadir}/icons/hicolor/86x86/apps/%{name}.png
%{_datadir}/icons/hicolor/108x108/apps/%{name}.png
%{_datadir}/icons/hicolor/128x128/apps/%{name}.png
%{_datadir}/icons/hicolor/172x172/apps/%{name}.png
