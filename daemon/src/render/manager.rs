//! Render thread: owns the X11 connection and the live popup stack (M3/M4).
//!
//! D-Bus handlers (async/tokio) send [`Command`]s over a channel; this thread
//! (the only owner of the non-`Send` cairo/X11 state) renders. Closures/actions
//! flow back over a [`Feedback`] channel to an async task that emits the FDO
//! signals. A 50 ms tick loop interleaves commands, X11 events, and expiry —
//! correct and simple; an `AsyncFd`/`select!` fast path is a later optimization.

use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{info, warn};
use x11rb::connection::Connection as _;
use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt as _, Window};
use x11rb::protocol::Event;

use super::cairo::{self, Card};
use super::x11::Ui;
use super::{Command, Feedback};
use crate::config::Config;
use crate::history::History;
use crate::notification::{Notification, NotificationId, Urgency};

/// FDO close reasons (org.freedesktop.Notifications spec).
pub mod reason {
    pub const EXPIRED: u32 = 1;
    pub const DISMISSED: u32 = 2;
    pub const CLOSED_BY_CALL: u32 = 3;
}

/// Fade animation state of a popup.
#[derive(Clone, Copy)]
enum Fade {
    /// Fading in: opacity ramps 0→1 from this instant.
    In(Instant),
    /// Fully shown.
    Visible,
    /// Fading out (opacity 1→0) from this instant; destroyed (with `reason`) when done.
    Out(Instant, u32),
}

/// One on-screen popup and its cached pixels (for Expose redraws).
struct Popup {
    id: NotificationId,
    window: Window,
    height: u16,
    stride: i32,
    pixels: Vec<u8>,
    expires_at: Option<Instant>,
    /// The `default` action key, if the notification declared one (whole-card click).
    default_action: Option<String>,
    fade: Fade,
}

struct Manager {
    ui: Ui,
    config: Config,
    /// Primary monitor geometry `(x, y, w, h)`.
    mon: (i16, i16, u16, u16),
    /// Newest first.
    popups: Vec<Popup>,
    /// Back-channel to the async signal emitter (NotificationClosed/ActionInvoked).
    feedback: UnboundedSender<Feedback>,
    /// DND or screen lock active (from the suppression watcher).
    dnd_lock: bool,
    /// A fullscreen window is focused (checked on the X11 side, throttled).
    fullscreen: bool,
    /// Last effective suppression state (for edge-triggered replay).
    was_suppressed: bool,
    /// Notifications received while suppressed, replayed on unsuppress (no loss).
    queued: Vec<Notification>,
    /// Persisted metadata-only history.
    history: History,
    /// Last fullscreen poll (throttle the X11 round-trips).
    last_fs_check: Instant,
}

impl Manager {
    fn new(
        ui: Ui,
        config: Config,
        mon: (i16, i16, u16, u16),
        feedback: UnboundedSender<Feedback>,
    ) -> Self {
        let history = History::load(config.history_size);
        Self {
            ui,
            config,
            mon,
            popups: Vec::new(),
            feedback,
            dnd_lock: false,
            fullscreen: false,
            was_suppressed: false,
            queued: Vec::new(),
            history,
            last_fs_check: Instant::now(),
        }
    }

    /// Effective suppression: DND/lock or a focused fullscreen window.
    fn suppressed(&self) -> bool {
        self.dnd_lock || self.fullscreen
    }

    /// Re-evaluate suppression; on the suppressed→unsuppressed edge, replay the queue.
    fn update_suppression(&mut self) -> Result<()> {
        let now = self.suppressed();
        if !now && self.was_suppressed {
            let queued = std::mem::take(&mut self.queued);
            info!(
                count = queued.len(),
                "unsuppressed — replaying queued notifications"
            );
            for n in queued {
                self.show(n)?;
            }
        }
        self.was_suppressed = now;
        Ok(())
    }

    /// Poll the focused-window fullscreen state (throttled), then re-evaluate.
    fn check_fullscreen(&mut self) -> Result<()> {
        if !self.config.suppress_on_fullscreen {
            return Ok(());
        }
        if self.last_fs_check.elapsed() < Duration::from_millis(400) {
            return Ok(());
        }
        self.last_fs_check = Instant::now();
        let fs = self.ui.active_window_fullscreen();
        if fs != self.fullscreen {
            self.fullscreen = fs;
            info!(fullscreen = fs, "fullscreen suppression changed");
            self.update_suppression()?;
        }
        Ok(())
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

    /// Render a card; returns `(pixels, stride, height)` with content-derived height.
    fn render_pixels(&self, n: &Notification) -> Result<(Vec<u8>, i32, u16)> {
        let icon = super::assets::load_icon(
            &n.app_icon,
            n.image_path.as_deref(),
            n.image_data.as_ref(),
            48,
        );
        let (pixels, stride, height) = cairo::render_card(&Card {
            summary: &n.summary,
            body: &n.body,
            width: self.config.width_px as i32,
            icon,
            font: &self.config.font_family,
            summary_pt: self.config.summary_size_pt,
            body_pt: self.config.body_size_pt,
        })?;
        Ok((pixels, stride, height as u16))
    }

    /// Notify clients of a close/action, if this is an FDO notification.
    fn emit(&self, fb: Feedback) {
        let _ = self.feedback.send(fb);
    }

    /// Queue a notification received while suppressed (DND/lock). Bounded to
    /// `max_stack * 2`, dropping the oldest non-critical entry on overflow.
    fn enqueue(&mut self, n: Notification) {
        self.queued.push(n);
        let cap = self.config.max_stack.saturating_mul(2).max(1);
        if self.queued.len() > cap {
            let drop_at = self
                .queued
                .iter()
                .position(|q| q.urgency != Urgency::Critical)
                .unwrap_or(0);
            self.queued.remove(drop_at);
        }
    }

    /// Set the DND/lock suppression state (from the watcher); replay on release.
    fn set_dnd_lock(&mut self, suppressed: bool) -> Result<()> {
        if self.dnd_lock != suppressed {
            self.dnd_lock = suppressed;
            info!(suppressed, "DND/lock suppression changed");
            self.update_suppression()?;
        }
        Ok(())
    }

    fn show(&mut self, n: Notification) -> Result<()> {
        if self.suppressed() {
            self.enqueue(n);
            return Ok(());
        }
        let expires_at = self.deadline(&n);
        let default_action = n.default_action.clone();
        let (pixels, stride, height) = self.render_pixels(&n)?;

        // replaces_id / dedup: update in place if the id is already displayed
        // (and not already fading out).
        if let Some(idx) = self
            .popups
            .iter()
            .position(|p| p.id == n.id && !matches!(p.fade, Fade::Out(..)))
        {
            let resize;
            {
                let p = &mut self.popups[idx];
                p.pixels = pixels;
                p.stride = stride;
                p.expires_at = expires_at;
                p.default_action = default_action;
                resize = p.height != height;
                p.height = height;
            }
            if resize {
                let win = self.popups[idx].window;
                self.ui
                    .conn
                    .configure_window(win, &ConfigureWindowAux::new().height(height as u32))?;
            }
            let p = &self.popups[idx];
            self.ui.put_argb(p.window, p.height, p.stride, &p.pixels)?;
            info!(?n.id, "popup updated in place");
            self.reflow()?;
            return Ok(());
        }

        // Drop the oldest when at capacity (M3-remaining: an "N more" overflow card).
        while self.popups.len() >= self.config.max_stack {
            if let Some(old) = self.popups.pop() {
                self.ui.conn.destroy_window(old.window)?;
                if let NotificationId::Fdo(id) = old.id {
                    self.emit(Feedback::Closed {
                        id,
                        reason: reason::EXPIRED,
                    });
                }
            }
        }

        let x = self.popup_x();
        let y = self.mon.1 + self.config.margin_px as i16;
        let window = self.ui.create_popup(x, y, self.config.width_px, height)?;

        // Start transparent and fade in (if enabled), so the popup doesn't flash.
        let fade = if self.config.fade_ms > 0 {
            self.ui.set_opacity(window, 0.0)?;
            Fade::In(Instant::now())
        } else {
            Fade::Visible
        };
        self.ui.map(window)?;
        self.ui.put_argb(window, height, stride, &pixels)?;

        // Best-effort sound on display (so DND/queued notifications stay silent).
        if !n.suppress_sound && (n.sound_file.is_some() || n.sound_name.is_some()) {
            self.emit(Feedback::PlaySound {
                file: n.sound_file.clone(),
                name: n.sound_name.clone(),
            });
        }

        self.popups.insert(
            0,
            Popup {
                id: n.id.clone(),
                window,
                height,
                stride,
                pixels,
                expires_at,
                default_action,
                fade,
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

    /// Begin closing a popup: start its fade-out (or tear down immediately if
    /// fading is disabled). `advance_fades` finishes faded-out popups.
    fn close(&mut self, id: &NotificationId, reason: u32) -> Result<()> {
        if self.config.fade_ms == 0 {
            return self.finish_close(id, reason);
        }
        if let Some(p) = self.popups.iter_mut().find(|p| &p.id == id) {
            if !matches!(p.fade, Fade::Out(..)) {
                p.fade = Fade::Out(Instant::now(), reason);
            }
        }
        Ok(())
    }

    /// Tear down a popup: destroy the window, emit `NotificationClosed`, reflow.
    fn finish_close(&mut self, id: &NotificationId, reason: u32) -> Result<()> {
        if let Some(pos) = self.popups.iter().position(|p| &p.id == id) {
            let p = self.popups.remove(pos);
            self.ui.conn.destroy_window(p.window)?;
            if let NotificationId::Fdo(fid) = p.id {
                self.emit(Feedback::Closed { id: fid, reason });
            }
            info!(?id, reason, "popup closed");
            self.reflow()?;
        }
        Ok(())
    }

    /// Advance fade animations one tick: update each popup's opacity and finish
    /// any whose fade-out has completed.
    fn advance_fades(&mut self) -> Result<()> {
        if self.config.fade_ms == 0 {
            return Ok(());
        }
        let dur = Duration::from_millis(self.config.fade_ms).as_secs_f64();
        let now = Instant::now();
        let mut done_out: Vec<(NotificationId, u32)> = Vec::new();

        for p in &mut self.popups {
            match p.fade {
                Fade::In(start) => {
                    let t = (now - start).as_secs_f64() / dur;
                    if t >= 1.0 {
                        p.fade = Fade::Visible;
                        self.ui.set_opacity(p.window, 1.0)?;
                    } else {
                        self.ui.set_opacity(p.window, t)?;
                    }
                }
                Fade::Out(start, reason) => {
                    let t = (now - start).as_secs_f64() / dur;
                    if t >= 1.0 {
                        done_out.push((p.id.clone(), reason));
                    } else {
                        self.ui.set_opacity(p.window, 1.0 - t)?;
                    }
                }
                Fade::Visible => {}
            }
        }
        for (id, reason) in done_out {
            self.finish_close(&id, reason)?;
        }
        Ok(())
    }

    /// Whether any popup is currently animating (so the loop ticks faster).
    fn has_active_fades(&self) -> bool {
        self.config.fade_ms > 0 && self.popups.iter().any(|p| !matches!(p.fade, Fade::Visible))
    }

    /// A click on a popup: invoke its default action (if any), then dismiss.
    fn click_window(&mut self, window: Window) -> Result<()> {
        let found = self
            .popups
            .iter()
            .find(|p| p.window == window)
            .map(|p| (p.id.clone(), p.default_action.clone()));
        if let Some((id, default_action)) = found {
            match (&id, default_action) {
                (NotificationId::Fdo(fid), Some(key)) => {
                    self.emit(Feedback::Action { id: *fid, key })
                }
                (NotificationId::Gtk { app_id, .. }, Some(action)) => {
                    self.emit(Feedback::GtkActivate {
                        app_id: app_id.clone(),
                        action,
                    })
                }
                _ => {}
            }
            self.close(&id, reason::DISMISSED)?;
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
            self.close(&id, reason::EXPIRED)?;
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
            // M4-remaining: hit-test against per-button layout regions; for now a
            // click invokes the default action (if any) and dismisses.
            Event::ButtonPress(e) => self.click_window(e.event)?,
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
pub fn run(
    mut rx: UnboundedReceiver<Command>,
    feedback: UnboundedSender<Feedback>,
    config: Config,
) -> Result<()> {
    let ui = Ui::connect()?;
    let mon = ui.primary_geometry()?;
    let mut mgr = Manager::new(ui, config, mon, feedback);
    info!(?mon, "render thread started");

    loop {
        loop {
            match rx.try_recv() {
                Ok(Command::Show(n)) => {
                    mgr.history.record(&n);
                    if let Err(e) = mgr.show(*n) {
                        warn!(error = %e, "failed to show popup");
                    }
                }
                Ok(Command::Close(id)) => {
                    if let Err(e) = mgr.close(&id, reason::CLOSED_BY_CALL) {
                        warn!(error = %e, "failed to close popup");
                    }
                }
                Ok(Command::SetSuppressed(s)) => {
                    if let Err(e) = mgr.set_dnd_lock(s) {
                        warn!(error = %e, "failed to apply suppression");
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
        mgr.advance_fades()?;
        mgr.check_fullscreen()?;

        // Tick faster while animating (≈60fps), idle otherwise.
        let tick = if mgr.has_active_fades() { 16 } else { 50 };
        std::thread::sleep(Duration::from_millis(tick));
    }
}
