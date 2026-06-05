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
    Show(Notification),
    /// Dismiss a notification by id (e.g. `CloseNotification`).
    Close(NotificationId),
    /// Tear down all popups and stop the render thread.
    Shutdown,
}

/// Chosen once at startup by probing (plan risk R5): the cairo XCBSurface fast
/// path (the one load-bearing unsafe seam) or the safe ImageSurface + put_image
/// fallback. A single decision, not per-window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    XcbSurface,
    ImageSurface,
}

/// Standalone visual smoke test (M2): render one ARGB override-redirect popup at
/// the top-right of the primary monitor, hold it briefly, then exit. Invoked via
/// `gnome-notistack --demo-popup`. Needs no D-Bus takeover.
pub fn demo() -> Result<()> {
    let ui = x11::Ui::connect()?;
    let (mx, my, mw, _mh) = ui.primary_geometry()?;
    let (w, h, margin) = (400u16, 110u16, 16i16);
    let x = mx + mw as i16 - w as i16 - margin;
    let y = my + margin;

    let win = ui.create_popup(x, y, w, h)?;
    let (pixels, stride) = cairo::render_card(&cairo::Card {
        summary: "gnome-notistack",
        body: "M2 demo — ARGB override-redirect popup drawn with cairo + pango.",
        width: w as i32,
        height: h as i32,
    })?;
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
