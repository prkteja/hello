mod camera;
mod pulse;
mod sound;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk::{gio, glib};

use pulse::{Device, Kind, Pulse, Snapshot};

const APP_ID: &str = "io.github.meetcheck";
const RECORD_SECONDS: u32 = 4;

fn main() -> glib::ExitCode {
    if let Err(e) = gst::init() {
        eprintln!("Failed to initialise GStreamer: {e}");
        return glib::ExitCode::FAILURE;
    }
    let app = gtk::Application::new(Some(APP_ID), gio::ApplicationFlags::empty());
    app.connect_activate(build_ui);
    app.run()
}

/// One row of controls for a speaker or microphone.
struct AudioRow {
    kind: Kind,
    dropdown: gtk::DropDown,
    model: gtk::StringList,
    volume: gtk::Scale,
    mute: gtk::ToggleButton,
    names: RefCell<Vec<String>>,
    /// Set while we push server state into the widgets, so their change
    /// handlers don't echo it back.
    updating: Cell<bool>,
}

impl AudioRow {
    fn new(kind: Kind) -> Rc<Self> {
        let model = gtk::StringList::new(&[]);
        let dropdown = gtk::DropDown::new(Some(model.clone()), gtk::Expression::NONE);
        dropdown.set_hexpand(true);
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.05);
        volume.set_draw_value(false);
        volume.set_size_request(120, -1);
        volume.set_tooltip_text(Some("Volume"));
        let mute = gtk::ToggleButton::new();
        mute.set_icon_name(match kind {
            Kind::Sink => "audio-volume-muted-symbolic",
            Kind::Source => "microphone-sensitivity-muted-symbolic",
        });
        mute.set_tooltip_text(Some("Mute"));
        Rc::new(Self {
            kind,
            dropdown,
            model,
            volume,
            mute,
            names: RefCell::default(),
            updating: Cell::new(false),
        })
    }

    fn connect(self: &Rc<Self>, pulse: &Rc<Pulse>) {
        let this = self.clone();
        let p = pulse.clone();
        self.dropdown.connect_selected_notify(move |dd| {
            if this.updating.get() {
                return;
            }
            if let Some(name) = this.names.borrow().get(dd.selected() as usize) {
                p.set_default(this.kind, name);
            }
        });
        let this = self.clone();
        let p = pulse.clone();
        self.volume.connect_value_changed(move |s| {
            if !this.updating.get()
                && let Some(name) = this.current()
            {
                p.set_volume(this.kind, &name, s.value());
            }
        });
        let this = self.clone();
        let p = pulse.clone();
        self.mute.connect_toggled(move |b| {
            if !this.updating.get()
                && let Some(name) = this.current()
            {
                p.set_mute(this.kind, &name, b.is_active());
            }
        });
    }

    fn current(&self) -> Option<String> {
        self.names.borrow().get(self.dropdown.selected() as usize).cloned()
    }

    fn update(&self, devices: &[Device], default: &str) {
        self.updating.set(true);
        let names: Vec<String> = devices.iter().map(|d| d.name.clone()).collect();
        if *self.names.borrow() != names {
            let labels: Vec<&str> = devices.iter().map(|d| d.description.as_str()).collect();
            self.model.splice(0, self.model.n_items(), &labels);
            *self.names.borrow_mut() = names;
        }
        let idx = devices.iter().position(|d| d.name == default);
        let has = idx.is_some();
        self.dropdown.set_selected(idx.map_or(gtk::INVALID_LIST_POSITION, |i| i as u32));
        if let Some(d) = idx.map(|i| &devices[i]) {
            self.volume.set_value(d.volume);
            self.mute.set_active(d.muted);
        }
        self.dropdown.set_sensitive(!devices.is_empty());
        self.volume.set_sensitive(has);
        self.mute.set_sensitive(has);
        self.updating.set(false);
    }
}

fn label(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l
}

fn build_ui(app: &gtk::Application) {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Meeting Check")
        .default_width(680)
        .default_height(640)
        .build();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 18);
    root.set_margin_top(18);
    root.set_margin_bottom(18);
    root.set_margin_start(18);
    root.set_margin_end(18);

    // ---- Camera ----------------------------------------------------------
    let picture = gtk::Picture::new();
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_vexpand(true);
    picture.set_size_request(-1, 280);
    let cam_status = gtk::Label::new(Some("Starting camera…"));
    cam_status.add_css_class("dim-label");
    cam_status.set_wrap(true);
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&picture));
    overlay.add_overlay(&cam_status);
    let frame = gtk::Frame::new(None);
    frame.set_child(Some(&overlay));
    root.append(&frame);

    let cameras = camera::list();
    let cam_labels: Vec<&str> = cameras.iter().map(|c| c.label.as_str()).collect();
    let cam_dropdown = gtk::DropDown::from_strings(&cam_labels);
    cam_dropdown.set_hexpand(true);
    cam_dropdown.set_sensitive(cameras.len() > 1);

    let preview: Rc<RefCell<Option<camera::Preview>>> = Rc::default();
    let start_camera = {
        let preview = preview.clone();
        let picture = picture.clone();
        let cam_status = cam_status.clone();
        move |idx: u32| {
            preview.take(); // stop the previous camera first
            picture.set_paintable(None::<&gtk::gdk::Paintable>);
            let Some(device) = cameras.get(idx as usize) else {
                cam_status.set_text("No camera found");
                cam_status.set_visible(true);
                return;
            };
            cam_status.set_text("Starting camera…");
            cam_status.set_visible(true);
            let pic = picture.clone();
            let status = cam_status.clone();
            let status_err = cam_status.clone();
            let pic_err = picture.clone();
            match camera::Preview::start(
                device,
                move |tex| {
                    status.set_visible(false);
                    pic.set_paintable(Some(&tex));
                },
                move |err| {
                    pic_err.set_paintable(None::<&gtk::gdk::Paintable>);
                    status_err.set_text(&format!("Camera error: {err}"));
                    status_err.set_visible(true);
                },
            ) {
                Ok(p) => *preview.borrow_mut() = Some(p),
                Err(e) => cam_status.set_text(&e),
            }
        }
    };
    start_camera(0);
    cam_dropdown.connect_selected_notify(move |dd| start_camera(dd.selected()));

    // ---- Audio -----------------------------------------------------------
    let speaker = AudioRow::new(Kind::Sink);
    let mic = AudioRow::new(Kind::Source);

    let test_btn = gtk::Button::with_label("Play test sound");
    let record_btn = gtk::Button::with_label("Record & play back");
    let level = gtk::LevelBar::new();
    level.set_hexpand(true);
    level.set_valign(gtk::Align::Center);
    // Keep the bar a single colour; the default thresholds read as warnings.
    level.remove_offset_value(Some(gtk::LEVEL_BAR_OFFSET_LOW));
    level.remove_offset_value(Some(gtk::LEVEL_BAR_OFFSET_HIGH));
    level.remove_offset_value(Some(gtk::LEVEL_BAR_OFFSET_FULL));
    level.set_tooltip_text(Some("Speak — the bar should move"));

    let grid = gtk::Grid::builder().row_spacing(10).column_spacing(10).build();
    grid.attach(&label("Camera"), 0, 0, 1, 1);
    grid.attach(&cam_dropdown, 1, 0, 4, 1);

    grid.attach(&label("Speaker"), 0, 1, 1, 1);
    grid.attach(&speaker.dropdown, 1, 1, 1, 1);
    grid.attach(&speaker.mute, 2, 1, 1, 1);
    grid.attach(&speaker.volume, 3, 1, 1, 1);
    grid.attach(&test_btn, 4, 1, 1, 1);

    grid.attach(&label("Microphone"), 0, 2, 1, 1);
    grid.attach(&mic.dropdown, 1, 2, 1, 1);
    grid.attach(&mic.mute, 2, 2, 1, 1);
    grid.attach(&mic.volume, 3, 2, 1, 1);
    grid.attach(&record_btn, 4, 2, 1, 1);

    grid.attach(&level, 1, 3, 3, 1);
    root.append(&grid);

    // Mic level meter. Restarted whenever the default mic changes.
    let meter: Rc<RefCell<Option<sound::MicMeter>>> = Rc::default();
    let restart_meter = {
        let meter = meter.clone();
        let level = level.clone();
        move || {
            meter.take();
            let level = level.clone();
            *meter.borrow_mut() = sound::MicMeter::start(move |v| level.set_value(v));
        }
    };
    restart_meter();

    let last_source = RefCell::new(String::new());
    let sp = speaker.clone();
    let mc = mic.clone();
    let pulse = Pulse::connect(move |snap: &Snapshot| {
        sp.update(&snap.sinks, &snap.default_sink);
        mc.update(&snap.sources, &snap.default_source);
        if *last_source.borrow() != snap.default_source {
            last_source.replace(snap.default_source.clone());
            restart_meter();
        }
    });
    match &pulse {
        Some(p) => {
            speaker.connect(p);
            mic.connect(p);
        }
        None => {
            for w in [&speaker.dropdown, &mic.dropdown] {
                w.set_sensitive(false);
                w.set_tooltip_text(Some("Could not connect to the sound server"));
            }
        }
    }

    // Speaker test.
    let playing: Rc<RefCell<Option<sound::OneShot>>> = Rc::default();
    {
        let playing = playing.clone();
        test_btn.connect_clicked(move |btn| {
            let b = btn.clone();
            let slot = playing.clone();
            match sound::play_test_sound(move |err| {
                slot.take();
                b.set_sensitive(true);
                b.set_label(if err.is_some() { "Playback failed — retry" } else { "Play test sound" });
            }) {
                Ok(p) => {
                    btn.set_sensitive(false);
                    btn.set_label("Playing…");
                    *playing.borrow_mut() = Some(p);
                }
                Err(_) => btn.set_label("Playback failed — retry"),
            }
        });
    }

    // Mic record & play back.
    let recorder: Rc<RefCell<Option<sound::OneShot>>> = Rc::default();
    {
        let recorder = recorder.clone();
        record_btn.connect_clicked(move |btn| {
            let path = glib::user_runtime_dir().join("meetcheck-mic-test.wav");
            let b = btn.clone();
            let slot = recorder.clone();
            let play_path = path.clone();
            let reset = |b: &gtk::Button, text: &str| {
                b.set_sensitive(true);
                b.set_label(text);
            };
            let result = sound::record(&path, move |err| {
                slot.take();
                if err.is_some() {
                    reset(&b, "Recording failed — retry");
                    return;
                }
                b.set_label("Playing back…");
                let b2 = b.clone();
                let slot2 = slot.clone();
                let file = play_path.clone();
                match sound::play_file(&play_path, move |err| {
                    slot2.take();
                    let _ = std::fs::remove_file(&file);
                    reset(&b2, if err.is_some() { "Playback failed — retry" } else { "Record & play back" });
                }) {
                    Ok(p) => *slot.borrow_mut() = Some(p),
                    Err(_) => reset(&b, "Playback failed — retry"),
                }
            });
            let Ok(rec) = result else {
                btn.set_label("Recording failed — retry");
                return;
            };
            *recorder.borrow_mut() = Some(rec);
            btn.set_sensitive(false);

            // Countdown, then stop recording (which triggers playback).
            let remaining = Cell::new(RECORD_SECONDS);
            btn.set_label(&format!("Speak now… {RECORD_SECONDS}"));
            let b = btn.clone();
            let rec = recorder.clone();
            glib::timeout_add_local(Duration::from_secs(1), move || {
                remaining.set(remaining.get() - 1);
                if remaining.get() > 0 {
                    b.set_label(&format!("Speak now… {}", remaining.get()));
                    return glib::ControlFlow::Continue;
                }
                b.set_label("Finishing…");
                if let Some(r) = rec.borrow().as_ref() {
                    r.finish();
                }
                glib::ControlFlow::Break
            });
        });
    }

    window.set_child(Some(&root));

    // Release camera and mic as soon as the window closes; keep the sound
    // server connection alive for as long as the window exists.
    window.connect_close_request(move |_| {
        let _ = &pulse;
        preview.take();
        meter.take();
        playing.take();
        recorder.take();
        glib::Propagation::Proceed
    });

    window.present();
}
