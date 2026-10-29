# Hello

A small GTK4 app for checking your setup before an online meeting:

- **Camera**: a live, mirrored preview. Switch cameras from the dropdown.
- **Speaker**: pick the default output, set volume or mute, and play a test sound.
- **Microphone**: pick the default input, set volume or mute, watch the level
  bar, and use *Record & play back* to hear yourself.

Changing a speaker or microphone sets the **system default**, so your meeting
app picks it up as well.

## Install from a release

Download `hello-<version>-x86_64-linux.tar.gz` from the
[releases page](https://github.com/prkteja/hello/releases), then:

```sh
tar xf hello-*-x86_64-linux.tar.gz
./hello-*/install.sh
```

Runtime requirements (preinstalled on most GNOME/Fedora desktops): GTK 4,
GStreamer with the base and good plugins, and PulseAudio or PipeWire.

## Build from source

You need Rust 1.88 or newer. Distro Rust packages are often older, so
[rustup](https://rustup.rs) is the easiest way to get it:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then install the build dependencies for your distro.

**Fedora**

```sh
sudo dnf install gcc pkgconf-pkg-config gtk4-devel pulseaudio-libs-devel \
    gstreamer1-devel gstreamer1-plugins-base-devel gstreamer1-plugins-good
```

**Ubuntu / Debian / Linux Mint / Pop!_OS** (Ubuntu 22.04 or newer)

```sh
sudo apt install build-essential pkg-config libgtk-4-dev libpulse-dev \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
    gstreamer1.0-plugins-good gstreamer1.0-pulseaudio
```

**Arch / Manjaro / EndeavourOS**

```sh
sudo pacman -S --needed base-devel gtk4 libpulse \
    gstreamer gst-plugins-base gst-plugins-good
```

Then build and install:

```sh
git clone https://github.com/prkteja/hello.git
cd hello
cargo build --release
./install.sh
```

## License

[MIT](LICENSE)
