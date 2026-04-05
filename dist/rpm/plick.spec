Name:           plick
Version:        0.1.0
Release:        1%{?dist}
Summary:        Minimal GIF screen recorder for Wayland
License:        MIT
URL:            https://github.com/plick

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gtk4-devel
BuildRequires:  glib2-devel
BuildRequires:  cairo-devel
BuildRequires:  pango-devel
BuildRequires:  graphene-devel
BuildRequires:  gdk-pixbuf2-devel

Requires:       gtk4
Requires:       ffmpeg
Requires:       xdg-desktop-portal
Requires:       pipewire

%description
Plick is a fast, minimal Peek-like GIF screen recorder for Wayland.
It uses xdg-desktop-portal for screen selection, records video via
FFmpeg from a PipeWire stream, then converts to high-quality GIF
using a 2-pass palette approach.

%prep
# Source is expected to be in the build directory

%build
cargo build --release

%install
install -Dm755 target/release/plick %{buildroot}%{_bindir}/plick
install -Dm644 data/com.github.plick.desktop %{buildroot}%{_datadir}/applications/com.github.plick.desktop
install -Dm644 data/com.github.plick.metainfo.xml %{buildroot}%{_datadir}/metainfo/com.github.plick.metainfo.xml
install -Dm644 data/com.github.plick.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/com.github.plick.svg

%files
%{_bindir}/plick
%{_datadir}/applications/com.github.plick.desktop
%{_datadir}/metainfo/com.github.plick.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/com.github.plick.svg
