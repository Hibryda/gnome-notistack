# Changelog

All notable changes to gnome-notistack are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Fixed
- Notifications no longer blink/flicker when an app rapidly closes and re-posts
  the same notification (Chromium update churn — Vivaldi, Discord/Electron send
  Notify→Close→Notify on update). A re-Notify now revives/updates the existing
  popup instead of spawning a duplicate, an app `CloseNotification` is debounced
  (~200 ms) so a re-Notify cancels it, and auto-expiry counts from **display
  time** rather than D-Bus receipt — so a suppression-queued (DND/lock/fullscreen)
  or overflow-held notification shows for its full duration instead of expiring
  instantly.
- Re-anchor popups when the monitor or resolution changes (not only on a config
  change).

### Changed
- Point the project URLs (`Cargo.toml` repository, the extension `url`, and the
  systemd unit `Documentation`) at the public GitHub repository.
- Correct the Debian package maintainer to `Hibryda <hibryda@protonmail.com>`.

## [0.1.0] - 2026-06-08

First public release.

### Added
- **Packaging & docs for public release** — MIT license, a public README with
  screenshots, a wiki, and a GitHub Actions release workflow that publishes `.deb`
  packages and portable tarballs (with a per-user installer) for X11/GNOME distros.
- **Configurable placement** — six positions (top/bottom × left/center/right;
  default top-right). Bottom placements anchor to the bottom edge and **stack
  upward** (newest at the bottom); the "+N more" tile follows the growth direction.
- **Freshest-notification accent** — a discrete Adwaita-blue stripe down the left
  edge of the newest popup, moving with the stack.
- **Screen-reader announcements (AT-SPI2 / Orca)** — a minimal accessible
  application registers on the a11y bus and emits `object:announcement` when a
  popup is shown, restoring the accessibility that override-redirect popups
  otherwise lose. Off by default (`a11y-announce`); announcements only (a
  navigable accessible tree is a later phase).
- **"+N more" overflow tile** — notifications beyond `max-stack` are held (not
  dropped) and shown as a clickable tile below the stack; the newest hidden one is
  promoted when a slot frees, hidden ones still expire, and clicking the tile
  dismisses them all.
- **`monitor = focused`** — show popups on the monitor currently under the pointer
  (a fresh batch follows focus; a live stack stays put).
- **Auto re-takeover after a daemon restart** — the extension watches the daemon's
  control name and re-runs the takeover when it reappears.
- Notification **action buttons** rendered as a full-width bottom bar (FDO
  `actions` + GTK `buttons`), clickable with hit-testing → `ActionInvoked` /
  `ActivateAction`.
- **Hover effects** — highlight on the hovered button/link + a hand cursor.
- Body **hyperlinks** (`<a href>`, clickable → `xdg-open`), inline **images**
  (`<img>`), and **line breaks** (`<br>` / `\n`).
- **GSettings-backed config + an Adwaita preferences window** (Appearance /
  Layout / Timing / Behavior), applied live (no restart).
- **Theme-aware** colors (Adwaita light/dark via `color-scheme`) + **system
  font**, **geometry-based width**, **monitor selection**, title↔body gap, and
  **min/max display-time caps**.
- **Mirror** shown notifications into GNOME's date-menu notification list.
- systemd **user unit** with queued autostart.

### Changed
- Configuration moved from TOML/CLI flags to **GSettings** (edited via the prefs
  window).
- `image-data` is validated at construction (`RawImage::from_wire`); colours are a
  clamping `Rgba` newtype.
- Icon + inline images are decoded once and cached per popup (hover re-renders no
  longer re-decode).
- Geometry math (`popup_width`/`anchor`) and the timeout floor/ceiling extracted
  into tested pure functions; unit tests 6 → 36 (incl. zvariant hint parsers and
  an adversarial markup torture test).

### Fixed
- **Session start:** survive X11 not being ready (self-heal `DISPLAY`/
  `XAUTHORITY` from systemctl + retry); exit on render-thread failure so systemd
  restarts cleanly rather than lingering render-less.
- Variable-height popups overlapping/gapping (running y-cursor); popups
  overlapping the top bar (`_NET_WORKAREA`); icons not resolving (use the active
  icon theme); prefs spin/combo rows silently changing on scroll.
- Cairo draw errors now surfaced (no silent blank popups); cosmetic fade/hover
  errors no longer kill the daemon; live-reload consistency (hover index reset,
  max-stack eviction, history-size, fade-disable); GTK default-action object-path
  mangling; `org.gnome.ScreenSaver` call timeout; many previously-silent failures
  now logged.

### Security
- Pango-markup injection via `<a>` link text (escape + card-height clamp).
- `parse_color` panic on multibyte input (reachable from the prefs colour fields).
- Render-thread crash on a NUL byte in a notification body/summary (interior NUL
  panicked pango's C-string conversion) — NULs are now stripped before pango.
- Mirror hardening (`useBodyMarkup: false`, themed-icon-only); `sound-file`
  restricted to existing files; notification summary no longer logged (PII).
