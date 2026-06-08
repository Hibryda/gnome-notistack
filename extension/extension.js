// extension.js — gnome-notistack takeover companion (GNOME Shell 48).
//
// Frees the notification bus names for the daemon and (on disable) restores the
// shell's handling. The mechanism lives in handshake.js.
//
// CRASH-SAFETY (the login SIGSEGV postmortem):
//   - enable() does NOT touch shell internals synchronously and does NOT unexport
//     the in-process daemon (that destabilized the shell mid-init).
//   - The takeover is DEFERRED past the fragile shell-init window, and
//     handshake.takeover() refuses to free any name unless our daemon is up and
//     ready to claim it. Together these prevent the "free names with no claimant
//     during init" path that segfaulted gnome-shell at login.

import GLib from 'gi://GLib';

import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Config from 'resource:///org/gnome/shell/misc/config.js';

import * as Handshake from './handshake.js';
import { Mirror } from './mirror.js';

// Validated live (M0.5): mid-session ReleaseName is safe and disable() restores
// cleanly (re-own, no Meta.restart). See docs/gnome48-audit.md "GTK path".
const ALLOW_GTK_TAKEOVER = true;

// Seconds to wait after enable() before attempting the takeover, to clear the
// shell-init window. The takeover additionally waits for the daemon to be ready.
const TAKEOVER_DELAY_SECONDS = 4;

export default class NotistackTakeoverExtension extends Extension {
    enable() {
        this._gtkTakenOver = false;
        this._takeoverTimeout = 0;
        this._cancelled = false;

        // Version guard: this technique is validated only against GNOME 48.x.
        const [major] = Config.PACKAGE_VERSION.split('.');
        if (major !== '48') {
            logError(new Error(
                `gnome-notistack: unsupported GNOME Shell ${Config.PACKAGE_VERSION} ` +
                `(targets 48.x); takeover skipped`));
            return;
        }

        // Mirror shown notifications into GNOME's notification list (date menu).
        this._mirror = new Mirror();
        this._mirror.enable();

        // Defer out of the shell-init window; the takeover itself is gated on the
        // daemon being ready, so it never destabilizes the shell. `_gtkTakenOver`
        // is set from the *result* so restore() only re-owns GTK if it was freed.
        this._takeoverTimeout = GLib.timeout_add_seconds(
            GLib.PRIORITY_DEFAULT, TAKEOVER_DELAY_SECONDS, () => {
                this._takeoverTimeout = 0;
                Handshake.takeover({
                    allowGtkTakeover: ALLOW_GTK_TAKEOVER,
                    cancelled: () => this._cancelled,
                })
                    .then(gtkTaken => {
                        if (!this._cancelled)
                            this._gtkTakenOver = gtkTaken;
                    })
                    .catch(e => logError(e, 'gnome-notistack: takeover failed'));
                return GLib.SOURCE_REMOVE;
            });
    }

    disable() {
        // Cancel any in-flight takeover so it can't free names after restore().
        this._cancelled = true;
        if (this._takeoverTimeout) {
            GLib.source_remove(this._takeoverTimeout);
            this._takeoverTimeout = 0;
        }
        if (this._mirror) {
            this._mirror.disable();
            this._mirror = null;
        }
        Handshake.restore({ gtkWasTakenOver: this._gtkTakenOver })
            .catch(e => logError(e, 'gnome-notistack: restore failed'));
        this._gtkTakenOver = false;
    }
}
