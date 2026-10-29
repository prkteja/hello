# Hello

A small GTK4 app for checking your setup before an online meeting:

- **Camera**: a live, mirrored preview. Switch cameras from the dropdown.
- **Speaker**: pick the default output, set volume or mute, and play a test sound.
- **Microphone**: pick the default input, set volume or mute, watch the level
  bar, and use *Record & play back* to hear yourself.

Changing a speaker or microphone sets the **system default**, so your meeting
app picks it up as well.

## Build

```sh
sudo dnf install gtk4-devel pulseaudio-libs-devel gstreamer1-devel gstreamer1-plugins-base-devel
cargo build --release
```

## Install (per user)

```sh
install -Dm755 target/release/hello ~/.local/bin/hello
install -Dm644 data/io.github.hello.desktop ~/.local/share/applications/io.github.hello.desktop
```
