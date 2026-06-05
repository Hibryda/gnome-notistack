//! Render thread: owns the X11 connection and the live popup stack (M3).
//!
//! D-Bus handlers (async/tokio) send [`Command`]s over a channel; this thread
//! (the only owner of the non-`Send` cairo/X11 state) renders. A 50 ms tick loop
//! interleaves commands, X11 events, and expiry — correct and simple; an
//! `AsyncFd`/`select!` fast path is a later optimization (rule 06).

use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::{info, warn};
use x11rb::connection::Connection as _;
use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt as _, Window};
use x11rb::protocol::Event;

use super::cairo::{self, Card};
use super::x11::Ui;
use super::Command;
use crate::config::Config;
use crate::notification::{Notification, NotificationId, Urgency};

/// One on-screen popup and its cached pixels (for Expose redraws).
struct Popup {
    id: NotificationId,
    window: Window,
    height: u16,
    stride: i32,
    pixels: Vec<u8>,
    expires_at: Option<Instant>,
}

struct Manager {
    ui: Ui,
    config: Config,
    /// Primary monitor geometry `(x, y, w, h)`.
    mon: (i16, i16, u16, u16),
    /// Newest first.
    popups: Vec<Popup>,
}

/// Fixed card height for M3; M4 measures the pango layout for a dynamic height.
const CARD_HEIGHT: u16 = 92;

impl Manager {
    fn new(ui: Ui, config: Config, mon: (i16, i16, u16, u16)) -> Self {
        Self {
            ui,
            config,
            mon,
            popups: Vec::new(),
        }
    }

    fn popup_x(&self) -> i16 {
        let margin = self.config.margin_px as i16;
        self.mon.0 + self.mon.2 as i16 - self.config.width_px as i16 - margin
    }

    fn deadline(&self, n: &Notification) -> Option<Instant> {
        if !n.auto_expires() {
            return None;
        }
        let ms = match n.expire_timeout_ms {
            Some(v) if v > 0 => v as u64,
            _ if n.urgency == Urgency::Low => self.config.low_urgency_timeout_ms,
            _ => self.config.default_timeout_ms,
        };
        Some(n.created + Duration::from_millis(ms))
    }

    fn render_pixels(&self, n: &Notification) -> Result<(Vec<u8>, i32)> {
        cairo::render_card(&Card {
            summary: &n.summary,
            body: &n.body,
            width: self.config.width_px as i32,
            height: CARD_HEIGHT as i32,
        })
    }

    fn show(&mut self, n: Notification) -> Result<()> {
        let expires_at = self.deadline(&n);
        let (pixels, stride) = self.render_pixels(&n)?;

        // replaces_id / dedup: update in place if the id is already displayed.
        if let Some(p) = self.popups.iter_mut().find(|p| p.id == n.id) {
            p.pixels = pixels;
            p.stride = stride;
            p.expires_at = expires_at;
            self.ui.put_argb(p.window, p.height, p.stride, &p.pixels)?;
            info!(?n.id, "popup updated in place");
            return Ok(());
        }

        // Drop the oldest when at capacity (M3-remaining: an "N more" overflow card).
        while self.popups.len() >= self.config.max_stack {
            if let Some(old) = self.popups.pop() {
                self.ui.conn.destroy_window(old.window)?;
            }
        }

        let x = self.popup_x();
        let y = self.mon.1 + self.config.margin_px as i16;
        let window = self
            .ui
            .create_popup(x, y, self.config.width_px, CARD_HEIGHT)?;
        self.ui.map(window)?;
        self.ui.put_argb(window, CARD_HEIGHT, stride, &pixels)?;
        self.popups.insert(
            0,
            Popup {
                id: n.id.clone(),
                window,
                height: CARD_HEIGHT,
                stride,
                pixels,
                expires_at,
            },
        );
        info!(?n.id, app = %n.app_name, count = self.popups.len(), "popup shown");
        self.reflow()
    }

    /// Reposition popups top-down (newest at top) with one batched flush.
    fn reflow(&mut self) -> Result<()> {
        let x = self.popup_x() as i32;
        let gap = self.config.gap_px as i32;
        let top = (self.mon.1 + self.config.margin_px as i16) as i32;
        for (i, p) in self.popups.iter().enumerate() {
            let y = top + i as i32 * (p.height as i32 + gap);
            self.ui
                .conn
                .configure_window(p.window, &ConfigureWindowAux::new().x(x).y(y))?;
        }
        self.ui.conn.flush()?;
        Ok(())
    }

    fn close(&mut self, id: &NotificationId) -> Result<()> {
        if let Some(pos) = self.popups.iter().position(|p| &p.id == id) {
            let p = self.popups.remove(pos);
            self.ui.conn.destroy_window(p.window)?;
            info!(?id, "popup closed");
            self.reflow()?;
        }
        Ok(())
    }

    fn close_window(&mut self, window: Window) -> Result<()> {
        if let Some(pos) = self.popups.iter().position(|p| p.window == window) {
            let id = self.popups[pos].id.clone();
            return self.close(&id);
        }
        Ok(())
    }

    fn expire_due(&mut self) -> Result<()> {
        let now = Instant::now();
        let expired: Vec<NotificationId> = self
            .popups
            .iter()
            .filter(|p| p.expires_at.map(|t| t <= now).unwrap_or(false))
            .map(|p| p.id.clone())
            .collect();
        for id in expired {
            self.close(&id)?;
        }
        Ok(())
    }

    fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Expose(e) => {
                if let Some(p) = self.popups.iter().find(|p| p.window == e.window) {
                    self.ui.put_argb(p.window, p.height, p.stride, &p.pixels)?;
                }
            }
            // M4: hit-test against layout click regions for action dispatch; for
            // now any click dismisses the clicked popup.
            Event::ButtonPress(e) => self.close_window(e.event)?,
            _ => {}
        }
        Ok(())
    }

    fn clear(&mut self) -> Result<()> {
        for p in self.popups.drain(..) {
            let _ = self.ui.conn.destroy_window(p.window);
        }
        self.ui.conn.flush()?;
        Ok(())
    }
}

/// Render-thread entry point. Owns the X11 connection for its whole lifetime.
pub fn run(mut rx: UnboundedReceiver<Command>, config: Config) -> Result<()> {
    let ui = Ui::connect()?;
    let mon = ui.primary_geometry()?;
    let mut mgr = Manager::new(ui, config, mon);
    info!(?mon, "render thread started");

    loop {
        loop {
            match rx.try_recv() {
                Ok(Command::Show(n)) => {
                    if let Err(e) = mgr.show(n) {
                        warn!(error = %e, "failed to show popup");
                    }
                }
                Ok(Command::Close(id)) => {
                    if let Err(e) = mgr.close(&id) {
                        warn!(error = %e, "failed to close popup");
                    }
                }
                Ok(Command::Shutdown) => {
                    mgr.clear()?;
                    return Ok(());
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    mgr.clear()?;
                    return Ok(());
                }
            }
        }

        while let Some(event) = mgr.ui.conn.poll_for_event()? {
            mgr.handle_event(event)?;
        }
        mgr.expire_due()?;

        std::thread::sleep(Duration::from_millis(50));
    }
}
