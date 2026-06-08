#!/usr/bin/env bash
#
# Portable per-user installer for gnome-notistack (no root, no .deb).
# Run from inside an unpacked release tarball:  ./install.sh
#
# Installs the daemon, the systemd *user* unit, and the GNOME shell extension
# into your home directory, compiles the GSettings schema, then enables both.
# Target: GNOME Shell 48 on X11. Uninstall:  ./install.sh --uninstall

set -euo pipefail

EXT_UUID="notistack@hemoglobina.store"
BIN_DIR="$HOME/.local/lib/gnome-notistack"
UNIT_DIR="$HOME/.config/systemd/user"
EXT_DIR="$HOME/.local/share/gnome-shell/extensions/$EXT_UUID"
HERE="$(cd "$(dirname "$0")" && pwd)"

uninstall() {
    systemctl --user disable --now gnome-notistack.service 2>/dev/null || true
    gnome-extensions disable "$EXT_UUID" 2>/dev/null || true
    rm -f "$UNIT_DIR/gnome-notistack.service"
    rm -rf "$BIN_DIR" "$EXT_DIR"
    systemctl --user daemon-reload 2>/dev/null || true
    echo "Uninstalled. (Log out / back in to fully unload the extension.)"
}

if [[ "${1:-}" == "--uninstall" ]]; then
    uninstall
    exit 0
fi

echo "Installing gnome-notistack for $USER…"

# 1. Daemon binary.
install -Dm755 "$HERE/gnome-notistack" "$BIN_DIR/gnome-notistack"

# 2. Extension (+ compile the GSettings schema).
mkdir -p "$EXT_DIR/schemas"
cp "$HERE"/extension/*.js "$HERE"/extension/metadata.json "$EXT_DIR/"
cp "$HERE"/extension/schemas/*.gschema.xml "$EXT_DIR/schemas/"
glib-compile-schemas "$EXT_DIR/schemas/"

# 3. systemd user unit — point ExecStart + the schema dir at the home install.
mkdir -p "$UNIT_DIR"
sed -e "s#^ExecStart=.*#ExecStart=$BIN_DIR/gnome-notistack#" \
    -e "s#^Environment=GSETTINGS_SCHEMA_DIR=.*#Environment=GSETTINGS_SCHEMA_DIR=$EXT_DIR/schemas#" \
    "$HERE/gnome-notistack.service" > "$UNIT_DIR/gnome-notistack.service"

# 4. Enable.
systemctl --user daemon-reload
systemctl --user enable --now gnome-notistack.service
gnome-extensions enable "$EXT_UUID" || true

cat <<EOF

Installed.
  • Daemon:    $BIN_DIR/gnome-notistack (systemd user service, autostarting)
  • Extension: $EXT_DIR

If notifications don't take over immediately, log out and back in (the extension
frees the bus names a few seconds after the shell starts). Configure via:
  gnome-extensions prefs $EXT_UUID
EOF
