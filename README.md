# Plick

A minimal screen-to-GIF recorder for Wayland desktops.

Plick uses xdg-desktop-portal for screen or window selection, records via GStreamer and PipeWire, and converts to high-quality GIF using FFmpeg's 2-pass palette approach. Recordings are automatically saved to a configurable output directory with timestamped filenames.

## Features

- Record full screens or individual windows
- Portal-based screen selection (works with GNOME, KDE, etc.)
- High-quality GIF output using 2-pass palette generation
- Configurable output directory (click the folder icon)
- Auto-saves as `screencast-YYYY-MM-DD-HHMMSS.gif`
- Lightweight toolbar UI
- **System tray icon** — appears while recording with a click-to-stop action
- **Remote stop** — run `plick --stop` to stop a recording from another terminal or a custom keyboard shortcut

## Runtime Dependencies

- **GStreamer 1.x** with `pipewiresrc` and `vp8enc` plugins
- **FFmpeg** (for GIF conversion)
- **PipeWire** (session daemon)
- **xdg-desktop-portal** with a working screencast backend

## Building from Source

### System Dependencies

**Fedora:**

```sh
sudo dnf install gcc cargo gtk4-devel gstreamer1-devel gstreamer1-plugins-base \
  gstreamer1-plugins-good ffmpeg pipewire-devel
```

**Ubuntu/Debian:**

```sh
sudo apt install build-essential cargo libgtk-4-dev libgstreamer1.0-dev \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good ffmpeg libpipewire-0.3-dev
```

**Arch:**

```sh
sudo pacman -S gcc cargo gtk4 gstreamer gst-plugins-base gst-plugins-good ffmpeg pipewire
```

### Build and Run

```sh
cargo build --release
./target/release/plick
```

### Install System-Wide

```sh
./dist/build.sh              # installs to /usr/local by default
./dist/build.sh /usr         # or specify a prefix
```

This installs the binary, desktop entry, appstream metadata, and icon.

## Building the Flatpak

### Prerequisites

Install Flatpak and flatpak-builder:

```sh
# Fedora
sudo dnf install flatpak flatpak-builder

# Ubuntu/Debian
sudo apt install flatpak flatpak-builder
```

Install the GNOME SDK and Rust extension:

```sh
flatpak install flathub org.gnome.Platform//47 org.gnome.Sdk//47
flatpak install flathub org.freedesktop.Sdk.Extension.rust-stable//24.08
```

### Vendor Dependencies

Flatpak builds are sandboxed with no network access. Rust dependencies must be vendored before building:

```sh
cargo vendor
```

This creates a `vendor/` directory and the project includes a `.cargo/config.toml` that tells Cargo to use it. Both are included in the Flatpak source bundle automatically.

### Build

```sh
flatpak-builder --force-clean build-dir dist/flatpak/com.github.plick.yml
```

### Install Locally

```sh
flatpak-builder --user --install --force-clean build-dir dist/flatpak/com.github.plick.yml
```

### Run

```sh
flatpak run com.github.plick
```

### Create a Distributable Bundle

To create a single `.flatpak` file you can share:

```sh
flatpak-builder --repo=repo --force-clean build-dir dist/flatpak/com.github.plick.yml
flatpak build-bundle repo plick.flatpak com.github.plick
```

Others can install it with:

```sh
flatpak install plick.flatpak
```

## Stopping a Recording

There are three ways to stop a recording:

1. **Stop button** — click "Stop" in the Plick toolbar
2. **Tray icon** — click the recording indicator in the system tray (requires the [AppIndicator](https://extensions.gnome.org/extension/615/appindicator-support/) GNOME Shell extension on GNOME desktops)
3. **CLI** — run `plick --stop` from any terminal to stop the running recording

### Global Keyboard Shortcut

You can bind `plick --stop` to a global keyboard shortcut so you can stop recording from anywhere:

**GNOME:** Settings > Keyboard > Custom Shortcuts > Add:
- Name: `Stop Plick Recording`
- Command: `plick --stop` (or the full path to the binary)
- Shortcut: your preferred key combination (e.g. `Ctrl+Shift+R`)

**KDE:** System Settings > Shortcuts > Custom Shortcuts > Add a new shortcut with `plick --stop` as the command.

## Configuration

Plick stores its configuration at `~/.config/plick/config.toml`:

```toml
output_dir = "/home/user/Videos"
capture_fps = 30
gif_fps = 15
max_duration_secs = 120
countdown_secs = 3
```

The output directory can also be changed at runtime using the folder icon in the toolbar.

## License

MIT
