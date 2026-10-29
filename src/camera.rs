//! Camera discovery and a live preview that hands frames to GTK as textures.

use gst::prelude::*;
use gtk::{gdk, glib};

pub struct Camera {
    pub label: String,
    device: gst::Device,
    infrared: bool,
}

impl Camera {
    /// Opens V4L2 nodes directly when we know them: it is the most reliable
    /// path outside a sandbox, while PipeWire may refuse unportaled clients.
    fn source(&self) -> Result<gst::Element, String> {
        let v4l2_path = self
            .device
            .properties()
            .and_then(|p| p.get::<String>("api.v4l2.path").ok());
        match v4l2_path {
            Some(path) => gst::ElementFactory::make("v4l2src")
                .property("device", path)
                .build()
                .map_err(|_| "Missing GStreamer element: v4l2src".into()),
            None => self
                .device
                .create_element(None)
                .map_err(|_| "Could not open camera".into()),
        }
    }
}

/// Lists cameras, regular ones first and infrared sensors last.
pub fn list() -> Vec<Camera> {
    let monitor = gst::DeviceMonitor::new();
    monitor.add_filter(Some("Video/Source"), None);
    if monitor.start().is_err() {
        return Vec::new();
    }
    let mut cameras: Vec<Camera> = monitor
        .devices()
        .into_iter()
        .map(|device| {
            // IR face-unlock sensors only offer grayscale formats.
            let infrared = device.caps().is_some_and(|caps| {
                caps.iter().all(|s| {
                    s.name() != "image/jpeg"
                        && s.get::<&str>("format").is_ok_and(|f| f.starts_with("GRAY") || f == "DMA_DRM")
                })
            });
            let name = device.display_name();
            let name = name.trim_end_matches(" (V4L2)").replace('_', " ");
            let label = if infrared { format!("{name} (infrared)") } else { name };
            Camera { label, device, infrared }
        })
        .collect();
    monitor.stop();
    cameras.sort_by_key(|c| c.infrared);
    cameras
}

struct Frame {
    width: i32,
    height: i32,
    stride: usize,
    data: glib::Bytes,
}

pub struct Preview {
    pipeline: gst::Pipeline,
    _watch: gst::bus::BusWatchGuard,
}

impl Preview {
    /// Starts streaming `camera`. Frames go to `on_frame` on the UI thread.
    pub fn start(
        camera: &Camera,
        on_frame: impl Fn(gdk::Texture) + 'static,
        on_error: impl Fn(String) + 'static,
    ) -> Result<Self, String> {
        let src = camera.source()?;
        let make = |name: &str| {
            gst::ElementFactory::make(name)
                .build()
                .map_err(|_| format!("Missing GStreamer element: {name}"))
        };
        let decode = make("decodebin")?;
        let convert = make("videoconvertscale")?;
        // Mirror, like every meeting app does for self-view.
        let flip = gst::ElementFactory::make("videoflip")
            .property_from_str("video-direction", "horiz")
            .build()
            .map_err(|_| "Missing GStreamer element: videoflip".to_string())?;
        let caps = gst::Caps::builder("video/x-raw")
            .field("format", "RGBA")
            .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
            .field("width", gst::IntRange::new(1, 1280))
            .build();
        let sink = gst_app::AppSink::builder()
            .caps(&caps)
            .max_buffers(1)
            .drop(true)
            .sync(false)
            .build();

        let pipeline = gst::Pipeline::new();
        pipeline
            .add_many([&src, &decode, &convert, &flip, sink.upcast_ref()])
            .map_err(|e| e.to_string())?;
        src.link(&decode).map_err(|e| e.to_string())?;
        gst::Element::link_many([&convert, &flip, sink.upcast_ref()]).map_err(|e| e.to_string())?;

        let convert_weak = convert.downgrade();
        decode.connect_pad_added(move |_, pad| {
            if let Some(convert) = convert_weak.upgrade() {
                let sinkpad = convert.static_pad("sink").unwrap();
                if !sinkpad.is_linked() {
                    let _ = pad.link(&sinkpad);
                }
            }
        });

        // Bounded to one frame: if the UI falls behind we simply skip frames.
        let (tx, rx) = async_channel::bounded::<Frame>(1);
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    let info = sample
                        .caps()
                        .and_then(|c| gst_video::VideoInfo::from_caps(c).ok())
                        .ok_or(gst::FlowError::NotNegotiated)?;
                    let buffer = sample.buffer_owned().ok_or(gst::FlowError::Error)?;
                    let map = buffer
                        .into_mapped_buffer_readable()
                        .map_err(|_| gst::FlowError::Error)?;
                    let _ = tx.try_send(Frame {
                        width: info.width() as i32,
                        height: info.height() as i32,
                        stride: info.stride()[0] as usize,
                        data: glib::Bytes::from_owned(map),
                    });
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        glib::spawn_future_local(async move {
            while let Ok(f) = rx.recv().await {
                let tex = gdk::MemoryTexture::new(
                    f.width,
                    f.height,
                    gdk::MemoryFormat::R8g8b8a8,
                    &f.data,
                    f.stride,
                );
                on_frame(tex.upcast());
            }
        });

        let watch = pipeline
            .bus()
            .unwrap()
            .add_watch_local(move |_, msg| {
                if let gst::MessageView::Error(e) = msg.view() {
                    on_error(e.error().to_string());
                }
                glib::ControlFlow::Continue
            })
            .map_err(|e| e.to_string())?;

        pipeline
            .set_state(gst::State::Playing)
            .map_err(|_| "Camera is unavailable (is another app using it?)".to_string())?;
        Ok(Self { pipeline, _watch: watch })
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        // Tearing down the pipeline also closes the frame channel, ending the
        // receiver task above.
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
