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

```sh
sudo dnf install gtk4-devel pulseaudio-libs-devel gstreamer1-devel gstreamer1-plugins-base-devel
cargo build --release
./install.sh
```
