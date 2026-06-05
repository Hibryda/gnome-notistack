//! Lock / idle detection (M5).
//!
//! Uses `org.gnome.ScreenSaver` (`GetActive` + the `ActiveChanged` signal) — NOT
//! the X11 screensaver extension, which is unreliable under GNOME (plan OBJ-39,
//! risk R7). Notifications are queued while locked and replayed on unlock;
//! `urgency=critical` is always queued. Suppression overall = DND || fullscreen
//! || screen_locked.
