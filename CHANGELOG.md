# Changelog

All notable changes to gnome-notistack are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/); this project is pre-1.0 and not
yet versioned, so everything lives under Unreleased.

## [Unreleased]

### Added
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
- `image-data` is validated at construction (`RawImage::from_wire`).
- Geometry math (`popup_width`/`anchor`) and the timeout floor/ceiling extracted
  into tested pure functions; unit tests 6 → 30.

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
- Mirror hardening (`useBodyMarkup: false`, themed-icon-only); `sound-file`
  restricted to existing files; notification summary no longer logged (PII).
