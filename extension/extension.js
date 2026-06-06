// extension.js — gnome-notistack takeover companion (GNOME Shell 48).
//
// Frees the notification bus names for the daemon and (on disable) restores the
// shell's handling. The actual mechanism lives in handshake.js; this file owns
// the lifecycle, the version guard, and the field-access type guards.
//
// Field names confirmed by the M0.5 source audit (docs/gnome48-audit.md):
//   Main.notificationDaemon._fdoNotificationDaemon
//   Main.notificationDaemon._gtkNotificationDaemon

import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as Config from 'resource:///org/gnome/shell/misc/config.js';

import * as Handshake from './handshake.js';

// The M0.5 live spike confirmed mid-session ReleaseName keeps the shell stable
// and that disable() restores cleanly (re-own, no Meta.restart), so the GTK
// takeover is enabled. See docs/gnome48-audit.md "GTK path".
const ALLOW_GTK_TAKEOVER = true;

export default class NotistackTakeoverExtension extends Extension {
    enable() {
        this._gtkTakenOver = false;

        // Version guard: this technique is validated only against GNOME 48.x.
        const [major] = Config.PACKAGE_VERSION.split('.');
        if (major !== '48') {
            logError(new Error(
                `gnome-notistack: unsupported GNOME Shell ${Config.PACKAGE_VERSION} ` +
                `(this extension targets 48.x); takeover skipped`));
            return;
        }

        // Type-guard the shell internals before touching them (rule 02).
        const daemon = Main.notificationDaemon;
        if (!daemon || typeof daemon._fdoNotificationDaemon !== 'object') {
            logError(new Error(
                'gnome-notistack: Main.notificationDaemon internals not as expected; ' +
                'takeover skipped'));
            return;
        }

        // Hygiene: unexport the in-process Fdo object path so the shell stops
        // answering on it (the gjs proxy still owns the *name* until SIGTERM).
        try {
            daemon._fdoNotificationDaemon?._dbusImpl?.unexport?.();
        } catch (e) {
            logError(e, 'gnome-notistack: _fdoNotificationDaemon.unexport failed');
        }

        this._gtkTakenOver = ALLOW_GTK_TAKEOVER;
        Handshake.takeover({ allowGtkTakeover: ALLOW_GTK_TAKEOVER })
            .catch(e => logError(e, 'gnome-notistack: takeover failed'));
    }

    disable() {
        Handshake.restore({ gtkWasTakenOver: this._gtkTakenOver })
            .catch(e => logError(e, 'gnome-notistack: restore failed'));
        this._gtkTakenOver = false;
    }
}
