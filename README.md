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

- **GStreamer 1.x** with `pipewiresrc` and `vp8enc` plugins, plus the `gst-launch-1.0` command-line tool
- **FFmpeg** (for GIF conversion)
- **PipeWire** (session daemon)
- **xdg-desktop-portal** with a working screencast backend

If Plick shows a "Missing dependencies" dialog at startup, install the missing runtime packages for your distro:

**Fedora:**

```sh
sudo dnf install ffmpeg gstreamer1-tools gstreamer1-plugins-base gstreamer1-plugins-good
```

**Debian/Ubuntu:**

```sh
sudo apt install ffmpeg gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-pipewire
```

**Arch:**

```sh
sudo pacman -S ffmpeg gstreamer gst-plugins-base gst-plugins-good gst-plugin-pipewire
```

## Building from Source

### System Dependencies

**Fedora:**

```sh
sudo dnf install gcc cargo gtk4-devel gstreamer1-devel gstreamer1-tools \
  gstreamer1-plugins-base gstreamer1-plugins-good ffmpeg pipewire-devel
```

**Ubuntu/Debian:**

```sh
sudo apt install build-essential cargo libgtk-4-dev libgstreamer1.0-dev \
  gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-pipewire ffmpeg libpipewire-0.3-dev
```

**Arch:**

```sh
sudo pacman -S gcc cargo gtk4 gstreamer gst-plugins-base gst-plugins-good \
  gst-plugin-pipewire ffmpeg pipewire
```

### Build and Run

```sh
cargo build --release
./target/release/plick
```

#### Vendor issues
If you run into an error that says: `the listed checksum...` you probably need to do this:
```
rm -rf vendor/
cargo vendor
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
2. **Tray icon** — click the recording indicator in the system tray (see [Tray Icon Setup](#tray-icon-setup) below)
3. **CLI** — run `plick --stop` from any terminal to stop the running recording

### Global Keyboard Shortcut

You can bind `plick --stop` to a global keyboard shortcut so you can stop recording from anywhere:

**GNOME:** Settings > Keyboard > Custom Shortcuts > Add:
- Name: `Stop Plick Recording`
- Command: `plick --stop` (or the full path to the binary)
- Shortcut: your preferred key combination (e.g. `Ctrl+Shift+R`)

**KDE:** System Settings > Shortcuts > Custom Shortcuts > Add a new shortcut with `plick --stop` as the command.

## Tray Icon Setup

The tray icon uses the `StatusNotifierItem` D-Bus protocol. KDE Plasma supports this natively. GNOME requires an extension.

**Fedora:** The AppIndicator extension is typically pre-installed. If the tray icon is missing, enable it:

```sh
gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com
```

**Debian/Ubuntu:** The extension is not installed by default:

```sh
sudo apt install gnome-shell-extension-appindicator
gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com
```

A log-out/log-in is required if GNOME Shell doesn't pick it up immediately.

**Arch:**

```sh
sudo pacman -S gnome-shell-extension-appindicator
gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com
```

If the tray icon still doesn't appear, check that a `StatusNotifierWatcher` is running:

```sh
busctl --user list | grep StatusNotifier
```

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
