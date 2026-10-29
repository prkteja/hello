//! Audio device management through the PulseAudio API (served by PipeWire on
//! modern desktops). Runs on the GLib main loop, so callbacks land on the UI
//! thread.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::glib;
use libpulse_binding as pa;
use libpulse_glib_binding::Mainloop;
use pa::callbacks::ListResult;
use pa::context::subscribe::InterestMaskSet;
use pa::context::{Context, FlagSet, State};
use pa::volume::{ChannelVolumes, Volume};

#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub name: String,
    pub description: String,
    pub channels: u8,
    /// 1.0 = 100%
    pub volume: f64,
    pub muted: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub sinks: Vec<Device>,
    pub sources: Vec<Device>,
    pub default_sink: String,
    pub default_source: String,
}

#[derive(Clone, Copy)]
pub enum Kind {
    Sink,
    Source,
}

type Listener = Rc<dyn Fn(&Snapshot)>;

pub struct Pulse {
    _mainloop: Mainloop,
    ctx: Rc<RefCell<Context>>,
    snapshot: Rc<RefCell<Snapshot>>,
}

impl Pulse {
    /// Connects to the sound server; `on_change` runs whenever devices,
    /// defaults or volumes change.
    pub fn connect(on_change: impl Fn(&Snapshot) + 'static) -> Option<Rc<Self>> {
        let mainloop = Mainloop::new(None)?;
        let ctx = Rc::new(RefCell::new(Context::new(&mainloop, "Hello")?));
        let snapshot = Rc::new(RefCell::new(Snapshot::default()));
        let listener: Listener = Rc::new(on_change);
        let refresh_queued = Rc::new(Cell::new(false));

        let schedule_refresh = {
            let ctx = Rc::downgrade(&ctx);
            let snapshot = snapshot.clone();
            let listener = listener.clone();
            let queued = refresh_queued.clone();
            Rc::new(move || {
                // Events come in bursts; coalesce them into one refresh.
                if queued.replace(true) {
                    return;
                }
                let ctx = ctx.clone();
                let snapshot = snapshot.clone();
                let listener = listener.clone();
                let queued = queued.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(30), move || {
                    queued.set(false);
                    if let Some(ctx) = ctx.upgrade() {
                        refresh(&ctx.borrow(), snapshot, listener);
                    }
                });
            })
        };

        {
            let weak = Rc::downgrade(&ctx);
            let schedule_refresh = schedule_refresh.clone();
            ctx.borrow_mut().set_state_callback(Some(Box::new(move || {
                let Some(ctx) = weak.upgrade() else { return };
                // The callback can fire while the context is mutably borrowed.
                let Ok(mut c) = ctx.try_borrow_mut() else { return };
                if c.get_state() == State::Ready {
                    let schedule = schedule_refresh.clone();
                    c.set_subscribe_callback(Some(Box::new(move |_, _, _| schedule())));
                    c.subscribe(
                        InterestMaskSet::SINK | InterestMaskSet::SOURCE | InterestMaskSet::SERVER,
                        |_| {},
                    );
                    drop(c);
                    schedule_refresh();
                }
            })));
        }

        ctx.borrow_mut()
            .connect(None, FlagSet::NOFAIL, None)
            .ok()?;

        Some(Rc::new(Self { _mainloop: mainloop, ctx, snapshot }))
    }

    pub fn set_default(&self, kind: Kind, name: &str) {
        let mut ctx = self.ctx.borrow_mut();
        match kind {
            Kind::Sink => ctx.set_default_sink(name, |_| {}),
            Kind::Source => ctx.set_default_source(name, |_| {}),
        };
    }

    pub fn set_volume(&self, kind: Kind, name: &str, volume: f64) {
        let channels = {
            let snap = self.snapshot.borrow();
            let list = match kind {
                Kind::Sink => &snap.sinks,
                Kind::Source => &snap.sources,
            };
            match list.iter().find(|d| d.name == name) {
                Some(d) => d.channels,
                None => return,
            }
        };
        let raw = (volume.clamp(0.0, 1.5) * Volume::NORMAL.0 as f64) as u32;
        let mut cv = ChannelVolumes::default();
        cv.set(channels, Volume(raw));
        let mut introspect = self.ctx.borrow().introspect();
        match kind {
            Kind::Sink => introspect.set_sink_volume_by_name(name, &cv, None),
            Kind::Source => introspect.set_source_volume_by_name(name, &cv, None),
        };
    }

    pub fn set_mute(&self, kind: Kind, name: &str, mute: bool) {
        let mut introspect = self.ctx.borrow().introspect();
        match kind {
            Kind::Sink => introspect.set_sink_mute_by_name(name, mute, None),
            Kind::Source => introspect.set_source_mute_by_name(name, mute, None),
        };
    }
}

fn volume_of(cv: &ChannelVolumes) -> f64 {
    cv.avg().0 as f64 / Volume::NORMAL.0 as f64
}

/// Fetches server defaults, sinks and sources, then hands one consistent
/// snapshot to the listener.
fn refresh(ctx: &Context, snapshot: Rc<RefCell<Snapshot>>, listener: Listener) {
    let next = Rc::new(RefCell::new(Snapshot::default()));
    let pending = Rc::new(Cell::new(3));

    let finish = {
        let next = next.clone();
        Rc::new(move || {
            pending.set(pending.get() - 1);
            if pending.get() == 0 {
                let new = next.take();
                if *snapshot.borrow() != new {
                    *snapshot.borrow_mut() = new;
                    listener(&snapshot.borrow());
                }
            }
        })
    };

    let introspect = ctx.introspect();
    {
        let next = next.clone();
        let finish = finish.clone();
        introspect.get_server_info(move |info| {
            let mut n = next.borrow_mut();
            n.default_sink = info.default_sink_name.as_deref().unwrap_or("").to_owned();
            n.default_source = info.default_source_name.as_deref().unwrap_or("").to_owned();
            drop(n);
            finish();
        });
    }
    {
        let next = next.clone();
        let finish = finish.clone();
        introspect.get_sink_info_list(move |res| match res {
            ListResult::Item(i) => next.borrow_mut().sinks.push(Device {
                name: i.name.as_deref().unwrap_or("").to_owned(),
                description: i.description.as_deref().unwrap_or("Unknown").to_owned(),
                channels: i.volume.len(),
                volume: volume_of(&i.volume),
                muted: i.mute,
            }),
            ListResult::End | ListResult::Error => finish(),
        });
    }
    introspect.get_source_info_list(move |res| match res {
        ListResult::Item(i) => {
            // Skip "Monitor of ..." sources; they aren't microphones.
            if i.monitor_of_sink.is_none() {
                next.borrow_mut().sources.push(Device {
                    name: i.name.as_deref().unwrap_or("").to_owned(),
                    description: i.description.as_deref().unwrap_or("Unknown").to_owned(),
                    channels: i.volume.len(),
                    volume: volume_of(&i.volume),
                    muted: i.mute,
                });
            }
        }
        ListResult::End | ListResult::Error => finish(),
    });
}
