//! ARGB32 override-redirect rendering (M2 windows, M4 card drawing).
//!
//! One override-redirect ARGB32 window per popup; cairo/pango drawing; y-cursor
//! layout; freedesktop icon assets.

pub mod assets;
pub mod cairo;
pub mod layout;
pub mod manager;
pub mod window;
pub mod x11;

use anyhow::Result;
use std::time::{Duration, Instant};
use tracing::info;
use x11rb::connection::Connection as _;

use crate::notification::{Notification, NotificationId};

/// Messages from the D-Bus handlers (tokio) to the render thread.
pub enum Command {
    /// Show a new notification, or update one in place if its id already shows.
    /// Boxed: `Notification` is much larger than the other variants.
    Show(Box<Notification>),
    /// Dismiss a notification by id (e.g. `CloseNotification`).
    Close(NotificationId),
    /// Enter/leave suppression (DND or screen lock): queue while true, replay on false.
    SetSuppressed(bool),
    /// Tear down all popups and stop the render thread.
    Shutdown,
}

/// Messages from the render thread back to the async FDO signal emitter.
pub enum Feedback {
    /// Emit `NotificationClosed(id, reason)`.
    Closed { id: u32, reason: u32 },
    /// Emit `ActionInvoked(id, key)`.
    Action { id: u32, key: String },
    /// Play a notification sound (best-effort), by file path or themed name.
    PlaySound {
        file: Option<String>,
        name: Option<String>,
    },
}

/// Chosen once at startup by probing (plan risk R5): the cairo XCBSurface fast
/// path (the one load-bearing unsafe seam) or the safe ImageSurface + put_image
/// fallback. A single decision, not per-window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    XcbSurface,
    ImageSurface,
}

/// Standalone visual smoke test: render one ARGB override-redirect popup at the
/// top-right of the primary monitor (using the configured font/sizes), hold it
/// briefly, then exit. Invoked via `gnome-notistack --demo-popup`. No D-Bus.
pub fn demo(config: &crate::config::Config) -> Result<()> {
    let ui = x11::Ui::connect()?;
    let (mx, my, mw, _mh) = ui.primary_geometry()?;
    let (w, margin) = (config.width_px, config.margin_px as i16);
    let (pixels, stride, h) = cairo::render_card(&cairo::Card {
        summary: "gnome-notistack",
        body: "Demo — ARGB override-redirect popup drawn with cairo + pango.",
        width: w as i32,
        icon: None,
        font: &config.font_family,
        summary_pt: config.summary_size_pt,
        body_pt: config.body_size_pt,
    })?;
    let h = h as u16;
    let x = mx + mw as i16 - w as i16 - margin;
    let y = my + margin;

    let win = ui.create_popup(x, y, w, h)?;
    ui.map(win)?;
    ui.put_argb(win, h, stride, &pixels)?;
    info!(window = win, x, y, w, h, "demo popup mapped");

    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        while let Some(event) = ui.conn.poll_for_event()? {
            use x11rb::protocol::Event;
            match event {
                Event::Expose(_) => ui.put_argb(win, h, stride, &pixels)?,
                Event::ButtonPress(_) => {
                    info!("button press — closing demo popup");
                    return Ok(());
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    info!("demo popup timeout — exiting");
    Ok(())
}
