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
    AtomEnum, Colormap, ColormapAlloc, CreateGCAux, CreateWindowAux, EventMask, ImageFormat,
    PropMode, Screen, VisualClass, Visualid, Window, WindowClass,
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
        let opacity_atom = conn
            .intern_atom(false, b"_NET_WM_WINDOW_OPACITY")?
            .reply()?
            .atom;
        Ok(Self {
            conn,
            screen_num,
            root,
            visual_id,
            colormap,
            opacity_atom,
        })
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
                    | EventMask::LEAVE_WINDOW,
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
