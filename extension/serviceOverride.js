// serviceOverride.js — block D-Bus re-activation of the shell's notification
// proxy so it can't be respawned after we SIGTERM / release it.
//
// Dual-write (audit: broker is `dbus-daemon --systemd-activation`, so the
// systemd override is primary; the legacy .service is belt-and-suspenders):
//   1. ~/.config/systemd/user/org.gnome.Shell.Notifications.service.d/override.conf
//   2. ~/.local/share/dbus-1/services/org.gnome.Shell.Notifications.service  (Exec=/bin/false)
//
// Writes are SYNCHRONOUS Gio calls: the files are a few hundred bytes, so this is
// microseconds (no main-thread concern), and it avoids the Gio._promisify dance
// the *_async variants require. Failures throw and abort the takeover (rule 02).

import GLib from 'gi://GLib';
import Gio from 'gi://Gio';

const SYSTEMD_OVERRIDE_REL =
    'systemd/user/org.gnome.Shell.Notifications.service.d/override.conf';
const DBUS_SERVICE_REL = 'dbus-1/services/org.gnome.Shell.Notifications.service';

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

function writeFile(path, contents) {
    const file = Gio.File.new_for_path(path);
    try {
        file.get_parent().make_directory_with_parents(null);
    } catch (e) {
        if (!e.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.EXISTS))
            throw e;
    }
    file.replace_contents(
        new TextEncoder().encode(contents),
        null, // etag
        false, // make_backup
        Gio.FileCreateFlags.REPLACE_DESTINATION,
        null); // cancellable
}

/** Install both blocking overrides. Throws on failure. */
export function install() {
    writeFile(configPath(), OVERRIDE_CONTENTS);
    writeFile(dataPath(), DBUS_SERVICE_CONTENTS);
}

/** Remove both overrides (restore on disable()). Best-effort; missing is fine. */
export function remove() {
    for (const path of [configPath(), dataPath()]) {
        const file = Gio.File.new_for_path(path);
        try {
            file.delete(null);
        } catch (e) {
            if (!e.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.NOT_FOUND))
                throw e;
        }
    }
}
