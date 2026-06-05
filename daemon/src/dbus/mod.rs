//! D-Bus surface: the two notification protocols plus the private control iface.
//!
//! **Ownership invariant** (see `docs/gnome48-audit.md`): the daemon requests
//! these names `REPLACE_EXISTING` only — never `ALLOW_REPLACEMENT`, never
//! `DO_NOT_QUEUE` — so it queues and D-Bus promotes it when the companion
//! extension releases the shell's ownership (`ReleaseName`).

pub mod fdo;
pub mod gtk;
pub mod private;

/// `org.freedesktop.Notifications` — the standard FDO name (gjs proxy today).
pub const FDO_NAME: &str = "org.freedesktop.Notifications";
/// `org.gtk.Notifications` — GNOME's private name (main gnome-shell today).
pub const GTK_NAME: &str = "org.gtk.Notifications";

pub const FDO_PATH: &str = "/org/freedesktop/Notifications";
pub const GTK_PATH: &str = "/org/gtk/Notifications";
