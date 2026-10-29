//! Short-lived GStreamer pipelines for testing speakers and microphones.
//! Everything plays to / records from the *default* device, so whatever the
//! user picks in the dropdowns is exactly what gets tested.

use std::cell::Cell;
use std::path::Path;

use gst::prelude::*;
use gtk::glib;

/// A pipeline that runs until end-of-stream or error, then reports back.
pub struct OneShot {
    pipeline: gst::Pipeline,
    _watch: gst::bus::BusWatchGuard,
}

impl OneShot {
    /// `on_done` gets `None` on success or an error message.
    pub fn start(
        desc: &str,
        setup: impl FnOnce(&gst::Pipeline),
        on_done: impl FnOnce(Option<String>) + 'static,
    ) -> Result<Self, String> {
        let pipeline = gst::parse::launch(desc)
            .map_err(|e| e.to_string())?
            .downcast::<gst::Pipeline>()
            .map_err(|_| "not a pipeline".to_string())?;
        setup(&pipeline);

        let on_done = Cell::new(Some(on_done));
        let watch = pipeline
            .bus()
            .unwrap()
            .add_watch_local(move |_, msg| {
                let result = match msg.view() {
                    gst::MessageView::Eos(_) => None,
                    gst::MessageView::Error(e) => Some(e.error().to_string()),
                    _ => return glib::ControlFlow::Continue,
                };
                if let Some(done) = on_done.take() {
                    // Run outside the bus callback so the owner may drop us.
                    glib::idle_add_local_once(move || done(result));
                }
                glib::ControlFlow::Continue
            })
            .map_err(|e| e.to_string())?;

        pipeline
            .set_state(gst::State::Playing)
            .map_err(|_| "could not start audio".to_string())?;
        Ok(Self { pipeline, _watch: watch })
    }

    /// Ask a live pipeline (e.g. a recording) to wrap up cleanly.
    pub fn finish(&self) {
        self.pipeline.send_event(gst::event::Eos::new());
    }
}

impl Drop for OneShot {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

/// Three soft beeps on the default speaker.
pub fn play_test_sound(on_done: impl FnOnce(Option<String>) + 'static) -> Result<OneShot, String> {
    OneShot::start(
        "audiotestsrc wave=ticks freq=660 tick-interval=400000000 sine-periods-per-tick=120 \
         apply-tick-ramp=true volume=0.5 samplesperbuffer=4410 num-buffers=11 \
         ! audio/x-raw,rate=44100,channels=2 ! audioconvert ! pulsesink",
        |_| {},
        on_done,
    )
}

/// Records the default microphone into a WAV file until `finish()` is called.
pub fn record(path: &Path, on_done: impl FnOnce(Option<String>) + 'static) -> Result<OneShot, String> {
    OneShot::start(
        "pulsesrc ! audioconvert ! audio/x-raw,format=S16LE ! wavenc ! filesink name=out",
        |p| p.by_name("out").unwrap().set_property("location", path.to_str().unwrap_or_default()),
        on_done,
    )
}

pub fn play_file(path: &Path, on_done: impl FnOnce(Option<String>) + 'static) -> Result<OneShot, String> {
    OneShot::start(
        "filesrc name=in ! wavparse ! audioconvert ! audioresample ! pulsesink",
        |p| p.by_name("in").unwrap().set_property("location", path.to_str().unwrap_or_default()),
        on_done,
    )
}

/// Continuously measures the default microphone's input level.
pub struct MicMeter {
    pipeline: gst::Pipeline,
    _watch: gst::bus::BusWatchGuard,
}

impl MicMeter {
    /// `on_level` receives a value in 0.0..=1.0 roughly 20 times a second.
    pub fn start(on_level: impl Fn(f64) + 'static) -> Option<Self> {
        let pipeline = gst::parse::launch(
            "pulsesrc ! audioconvert ! level interval=50000000 post-messages=true ! fakesink sync=false",
        )
        .ok()?
        .downcast::<gst::Pipeline>()
        .ok()?;

        let watch = pipeline
            .bus()?
            .add_watch_local(move |_, msg| {
                if let gst::MessageView::Element(e) = msg.view()
                    && let Some(s) = e.structure()
                    && s.name() == "level"
                    && let Ok(rms) = s.get::<glib::ValueArray>("rms")
                {
                    let db = rms
                        .iter()
                        .filter_map(|v| v.get::<f64>().ok())
                        .fold(f64::NEG_INFINITY, f64::max);
                    // Map -60..-10 dBFS RMS onto the bar: a quiet room sits near
                    // the left, normal speech fills most of it.
                    on_level(((db + 60.0) / 50.0).clamp(0.0, 1.0));
                }
                glib::ControlFlow::Continue
            })
            .ok()?;

        pipeline.set_state(gst::State::Playing).ok()?;
        Some(Self { pipeline, _watch: watch })
    }
}

impl Drop for MicMeter {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
