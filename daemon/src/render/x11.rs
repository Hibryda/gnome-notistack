//! X11 connection, ARGB32 override-redirect window creation, EWMH, and blitting (M2).
//!
//! Uses x11rb's libxcb-backed `XCBConnection` (the connection cairo's XCBSurface
//! seam will later share). Windows are `depth 32` TrueColor with `border_pixel(0)`
//! (mandatory for a non-default visual) and `override_redirect` so the WM never
//! manages them. EWMH marks them as above-everything notification windows that
//! keep the compositor (so ARGB alpha works).

use anyhow::{anyhow, Context, Result};
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, Colormap, ColormapAlloc, CreateGCAux, CreateWindowAux,
    EventMask, ImageFormat, PropMode, Screen, VisualClass, Visualid, Window, WindowClass,
};
use x11rb::wrapper::ConnectionExt as _;
use x11rb::xcb_ffi::XCBConnection;

pub struct Ui {
    pub conn: XCBConnection,
    pub screen_num: usize,
    pub root: Window,
    pub visual_id: Visualid,
    pub colormap: Colormap,
    /// Cached `_NET_WM_WINDOW_OPACITY` atom (set per-frame during fades).
    opacity_atom: u32,
    /// Cached atoms for the fullscreen-suppression check.
    active_window_atom: u32,
    wm_state_atom: u32,
    fullscreen_atom: u32,
    /// Cached `_NET_WORKAREA` atom (usable area excluding panels/docks).
    workarea_atom: u32,
    /// Hand cursor for hovering buttons/links (0 if unavailable).
    hand_cursor: u32,
}

impl Ui {
    pub fn connect() -> Result<Self> {
        let (conn, screen_num) = XCBConnection::connect(None).context("connect to X11")?;
        let (root, visual_id) = {
            let screen = &conn.setup().roots[screen_num];
            let visual = find_argb_visual(screen)
                .ok_or_else(|| anyhow!("no 32-bit TrueColor (ARGB) visual on screen"))?;
            (screen.root, visual)
        };
        let colormap = conn.generate_id().context("generate colormap id")?;
        conn.create_colormap(ColormapAlloc::NONE, colormap, root, visual_id)?
            .check()
            .context("create colormap")?;
        let intern =
            |name: &[u8]| -> Result<u32> { Ok(conn.intern_atom(false, name)?.reply()?.atom) };
        let opacity_atom = intern(b"_NET_WM_WINDOW_OPACITY")?;
        let active_window_atom = intern(b"_NET_ACTIVE_WINDOW")?;
        let wm_state_atom = intern(b"_NET_WM_STATE")?;
        let fullscreen_atom = intern(b"_NET_WM_STATE_FULLSCREEN")?;
        let workarea_atom = intern(b"_NET_WORKAREA")?;
        // Hand cursor (glyph XC_hand2 = 58) from the standard "cursor" font.
        let hand_cursor = {
            let font = conn.generate_id().unwrap_or(0);
            let cur = conn.generate_id().unwrap_or(0);
            if font != 0 && cur != 0 && conn.open_font(font, b"cursor").is_ok() {
                let ok = conn
                    .create_glyph_cursor(
                        cur,
                        font,
                        font,
                        58,
                        59,
                        0,
                        0,
                        0,
                        u16::MAX,
                        u16::MAX,
                        u16::MAX,
                    )
                    .is_ok();
                let _ = conn.close_font(font);
                if ok {
                    cur
                } else {
                    0
                }
            } else {
                0
            }
        };
        Ok(Self {
            conn,
            screen_num,
            root,
            visual_id,
            colormap,
            opacity_atom,
            active_window_atom,
            wm_state_atom,
            fullscreen_atom,
            workarea_atom,
            hand_cursor,
        })
    }

    /// Set (or clear) the hand cursor on a popup window for hover feedback.
    pub fn set_pointer(&self, win: Window, hand: bool) -> Result<()> {
        let cursor = if hand { self.hand_cursor } else { 0 };
        self.conn
            .change_window_attributes(win, &ChangeWindowAttributesAux::new().cursor(cursor))?;
        self.conn.flush()?;
        Ok(())
    }

    /// The desktop work area `(x, y, width, height)` from `_NET_WORKAREA` — the
    /// usable region excluding panels/docks (so popups start under the top bar).
    /// Returns `None` if the property is absent. (First workspace's rect.)
    pub fn workarea(&self) -> Option<(i32, i32, i32, i32)> {
        let reply = self
            .conn
            .get_property(
                false,
                self.root,
                self.workarea_atom,
                AtomEnum::CARDINAL,
                0,
                4,
            )
            .ok()?
            .reply()
            .ok()?;
        let v: Vec<u32> = reply.value32()?.collect();
        if v.len() < 4 {
            return None;
        }
        Some((v[0] as i32, v[1] as i32, v[2] as i32, v[3] as i32))
    }

    /// Whether the currently focused window is fullscreen (best-effort; false on
    /// any error, incl. BadWindow if the active window vanished).
    pub fn active_window_fullscreen(&self) -> bool {
        let Ok(active) = self.conn.get_property(
            false,
            self.root,
            self.active_window_atom,
            AtomEnum::WINDOW,
            0,
            1,
        ) else {
            return false;
        };
        let Some(win) = active
            .reply()
            .ok()
            .and_then(|r| r.value32().and_then(|mut v| v.next()))
        else {
            return false;
        };
        if win == 0 {
            return false;
        }
        let Ok(state) =
            self.conn
                .get_property(false, win, self.wm_state_atom, AtomEnum::ATOM, 0, 64)
        else {
            return false;
        };
        state
            .reply()
            .ok()
            .and_then(|r| r.value32().map(|v| v.collect::<Vec<_>>()))
            .map(|atoms| atoms.contains(&self.fullscreen_atom))
            .unwrap_or(false)
    }

    fn intern(&self, name: &[u8]) -> Result<u32> {
        Ok(self.conn.intern_atom(false, name)?.reply()?.atom)
    }

    /// Set per-window opacity (0.0–1.0) via `_NET_WM_WINDOW_OPACITY`; the
    /// compositor blends it, so fading is cheap. Used by the fade animation.
    pub fn set_opacity(&self, win: Window, opacity: f64) -> Result<()> {
        let value = (opacity.clamp(0.0, 1.0) * u32::MAX as f64) as u32;
        self.conn.change_property32(
            PropMode::REPLACE,
            win,
            self.opacity_atom,
            AtomEnum::CARDINAL,
            &[value],
        )?;
        self.conn.flush()?;
        Ok(())
    }

    /// Geometry `(x, y, width, height)` of the primary monitor (first if none is
    /// flagged primary), falling back to the whole screen. Used to anchor top-right.
    pub fn primary_geometry(&self) -> Result<(i16, i16, u16, u16)> {
        if let Ok(reply) = self.conn.randr_get_monitors(self.root, true)?.reply() {
            if let Some(m) = reply
                .monitors
                .iter()
                .find(|m| m.primary)
                .or_else(|| reply.monitors.first())
            {
                return Ok((m.x, m.y, m.width, m.height));
            }
        }
        let screen = &self.conn.setup().roots[self.screen_num];
        Ok((0, 0, screen.width_in_pixels, screen.height_in_pixels))
    }

    /// Resolve the RandR connector name of a monitor (e.g. "DP-1") from its atom.
    fn monitor_name(&self, atom: u32) -> String {
        self.conn
            .get_atom_name(atom)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| String::from_utf8_lossy(&r.name).into_owned())
            .unwrap_or_default()
    }

    /// Connected monitors as `(name, x, y, w, h)` — for the prefs dropdown / lookup.
    pub fn list_monitors(&self) -> Vec<(String, i16, i16, u16, u16)> {
        let mut out = Vec::new();
        if let Ok(cookie) = self.conn.randr_get_monitors(self.root, true) {
            if let Ok(reply) = cookie.reply() {
                for m in &reply.monitors {
                    out.push((self.monitor_name(m.name), m.x, m.y, m.width, m.height));
                }
            }
        }
        out
    }

    /// Geometry of the named monitor ("primary"/empty → primary), falling back to
    /// the primary monitor if the named connector isn't present.
    pub fn monitor_geometry(&self, name: &str) -> Result<(i16, i16, u16, u16)> {
        if name != "primary" && !name.is_empty() {
            if let Ok(reply) = self.conn.randr_get_monitors(self.root, true)?.reply() {
                if let Some(m) = reply
                    .monitors
                    .iter()
                    .find(|m| self.monitor_name(m.name) == name)
                {
                    return Ok((m.x, m.y, m.width, m.height));
                }
            }
        }
        self.primary_geometry()
    }

    /// Create (unmapped) an ARGB32 override-redirect popup with EWMH hints set.
    pub fn create_popup(&self, x: i16, y: i16, w: u16, h: u16) -> Result<Window> {
        let win = self.conn.generate_id().context("generate window id")?;
        let aux = CreateWindowAux::new()
            .background_pixel(0)
            .border_pixel(0) // mandatory with a non-default (ARGB) visual
            .colormap(self.colormap)
            .override_redirect(1)
            .event_mask(
                EventMask::EXPOSURE
                    | EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::ENTER_WINDOW
                    | EventMask::LEAVE_WINDOW
                    | EventMask::POINTER_MOTION,
            );
        self.conn
            .create_window(
                32,
                win,
                self.root,
                x,
                y,
                w,
                h,
                0,
                WindowClass::INPUT_OUTPUT,
                self.visual_id,
                &aux,
            )?
            .check()
            .context("create override-redirect window")?;
        self.set_ewmh(win)?;
        Ok(win)
    }

    fn set_ewmh(&self, win: Window) -> Result<()> {
        let wm_type = self.intern(b"_NET_WM_WINDOW_TYPE")?;
        let wm_type_notif = self.intern(b"_NET_WM_WINDOW_TYPE_NOTIFICATION")?;
        let wm_state = self.intern(b"_NET_WM_STATE")?;
        let wm_state_above = self.intern(b"_NET_WM_STATE_ABOVE")?;
        let bypass = self.intern(b"_NET_WM_BYPASS_COMPOSITOR")?;
        self.conn.change_property32(
            PropMode::REPLACE,
            win,
            wm_type,
            AtomEnum::ATOM,
            &[wm_type_notif],
        )?;
        self.conn.change_property32(
            PropMode::REPLACE,
            win,
            wm_state,
            AtomEnum::ATOM,
            &[wm_state_above],
        )?;
        // 2 = "keep compositing" (we need alpha), not 1 ("disable").
        self.conn
            .change_property32(PropMode::REPLACE, win, bypass, AtomEnum::CARDINAL, &[2])?;
        Ok(())
    }

    pub fn map(&self, win: Window) -> Result<()> {
        self.conn.map_window(win)?;
        self.conn.flush()?;
        Ok(())
    }

    /// Blit a premultiplied BGRA buffer (cairo ARGB32) to the window. `stride`
    /// gives the row length; the image width is `stride / 4`.
    pub fn put_argb(&self, win: Window, height: u16, stride: i32, data: &[u8]) -> Result<()> {
        let gc = self.conn.generate_id().context("generate gc id")?;
        self.conn.create_gc(gc, win, &CreateGCAux::new())?;
        let img_w = (stride / 4) as u16;
        self.conn
            .put_image(
                ImageFormat::Z_PIXMAP,
                win,
                gc,
                img_w,
                height,
                0,
                0,
                0,
                32,
                data,
            )
            .context("put_image")?;
        self.conn.free_gc(gc)?;
        self.conn.flush()?;
        Ok(())
    }
}

fn find_argb_visual(screen: &Screen) -> Option<Visualid> {
    screen
        .allowed_depths
        .iter()
        .filter(|d| d.depth == 32)
        .flat_map(|d| d.visuals.iter())
        .find(|v| v.class == VisualClass::TRUE_COLOR)
        .map(|v| v.visual_id)
}
