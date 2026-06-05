// serviceOverride.js — block D-Bus re-activation of the shell's notification
// proxies so they cannot be respawned after we SIGTERM / release them.
//
// Dual-write (audit: broker is `dbus-daemon --systemd-activation`, so the
// systemd override is primary; the legacy .service is belt-and-suspenders):
//   1. ~/.config/systemd/user/org.gnome.Shell.Notifications.service.d/override.conf
//   2. ~/.local/share/dbus-1/services/org.gnome.Shell.Notifications.service  (Exec=/bin/false)
//
// All writes use async Gio.File APIs; failures are surfaced to the daemon via
// ReportHandshakeEvent and abort the takeover (rule 02 — fail loud).

import GLib from 'gi://GLib';
import Gio from 'gi://Gio';

const SYSTEMD_OVERRIDE_REL =
    'systemd/user/org.gnome.Shell.Notifications.service.d/override.conf';
const DBUS_SERVICE_REL =
    'dbus-1/services/org.gnome.Shell.Notifications.service';

const OVERRIDE_CONTENTS = '[Service]\nExecStart=\nExecStart=/bin/false\n';
const DBUS_SERVICE_CONTENTS =
    '[D-BUS Service]\n' +
    'Name=org.gnome.Shell.Notifications\n' +
    'Exec=/bin/false\n';

function configPath() {
    return GLib.build_filenamev([GLib.get_user_config_dir(), SYSTEMD_OVERRIDE_REL]);
}

function dataPath() {
    return GLib.build_filenamev([GLib.get_user_data_dir(), DBUS_SERVICE_REL]);
}

async function writeAtomic(path, contents) {
    const file = Gio.File.new_for_path(path);
    const parent = file.get_parent();
    try {
        parent.make_directory_with_parents(null);
    } catch (e) {
        if (!e.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.EXISTS))
            throw e;
    }
    const bytes = new GLib.Bytes(new TextEncoder().encode(contents));
    await file.replace_contents_bytes_async(
        bytes, null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
}

/** Install both blocking overrides. Resolves when written; rejects on failure. */
export async function install() {
    await writeAtomic(configPath(), OVERRIDE_CONTENTS);
    await writeAtomic(dataPath(), DBUS_SERVICE_CONTENTS);
}

/** Remove both overrides (restore on disable()). Best-effort; missing is fine. */
export async function remove() {
    for (const path of [configPath(), dataPath()]) {
        const file = Gio.File.new_for_path(path);
        try {
            await file.delete_async(GLib.PRIORITY_DEFAULT, null);
        } catch (e) {
            if (!e.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.NOT_FOUND))
                throw e;
        }
    }
}
