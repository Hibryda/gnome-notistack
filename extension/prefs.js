// prefs.js — gnome-notistack preferences (Adw.PreferencesWindow).
//
// Edits the GSettings schema org.gnome.shell.extensions.notistack, which the
// daemon reads and re-applies live (no restart) — except `gtk-takeover`, which
// is structural and takes effect on the next daemon start.

import Adw from 'gi://Adw';
import Gdk from 'gi://Gdk';
import Gio from 'gi://Gio';
import Gtk from 'gi://Gtk';

import { ExtensionPreferences } from 'resource:///org/gnome/Shell/Extensions/js/extensions/prefs.js';

/** Stop a widget from changing on mouse-scroll (so scrolling the page over a
 *  SpinRow doesn't silently nudge its value). */
function ignoreScroll(widget) {
    const scroll = new Gtk.EventControllerScroll({
        flags: Gtk.EventControllerScrollFlags.BOTH_AXES,
        propagation_phase: Gtk.PropagationPhase.CAPTURE,
    });
    scroll.connect('scroll', () => true); // Gdk.EVENT_STOP
    widget.add_controller(scroll);
}

/** SpinRow bound manually (avoids uint/double <-> double binding mismatches). */
function addSpin(group, settings, key, title, opts) {
    const { lower, upper, step, digits = 0, isDouble = false, subtitle = '' } = opts;
    const row = new Adw.SpinRow({
        title,
        subtitle,
        digits,
        adjustment: new Gtk.Adjustment({ lower, upper, step_increment: step }),
    });
    ignoreScroll(row);
    const get = () => (isDouble ? settings.get_double(key) : settings.get_uint(key));
    const set = v => (isDouble ? settings.set_double(key, v) : settings.set_uint(key, Math.round(v)));
    row.set_value(get());
    row.connect('notify::value', () => {
        const v = row.get_value();
        if (v !== get())
            set(v);
    });
    settings.connect(`changed::${key}`, () => row.set_value(get()));
    group.add(row);
    return row;
}

function addSwitch(group, settings, key, title, subtitle = '') {
    const row = new Adw.SwitchRow({ title, subtitle });
    group.add(row);
    settings.bind(key, row, 'active', Gio.SettingsBindFlags.DEFAULT);
    return row;
}

function addEntry(group, settings, key, title) {
    const row = new Adw.EntryRow({ title });
    group.add(row);
    settings.bind(key, row, 'text', Gio.SettingsBindFlags.DEFAULT);
    return row;
}

function addMonitorCombo(group, settings) {
    const options = [['primary', 'Primary (follow)']];
    try {
        const monitors = Gdk.Display.get_default()?.get_monitors();
        const n = monitors?.get_n_items() ?? 0;
        for (let i = 0; i < n; i++) {
            const m = monitors.get_item(i);
            const conn = m.get_connector?.() || `monitor-${i}`;
            const desc = m.get_description?.() || '';
            options.push([conn, desc && desc !== conn ? `${conn} — ${desc}` : conn]);
        }
    } catch (_e) {
        // No display (rare in prefs); the Primary option still works.
    }
    const model = new Gtk.StringList();
    options.forEach(([, label]) => model.append(label));
    const keys = options.map(([k]) => k);
    const row = new Adw.ComboRow({ title: 'Monitor', model });
    ignoreScroll(row);
    const sync = () => {
        const i = keys.indexOf(settings.get_string('monitor'));
        row.set_selected(i >= 0 ? i : 0);
    };
    sync();
    row.connect('notify::selected', () => settings.set_string('monitor', keys[row.get_selected()]));
    settings.connect('changed::monitor', sync);
    group.add(row);
}

function addThemeCombo(group, settings) {
    const options = [['auto', 'Follow system'], ['light', 'Light'], ['dark', 'Dark']];
    const model = new Gtk.StringList();
    options.forEach(([, label]) => model.append(label));
    const keys = options.map(([k]) => k);
    const row = new Adw.ComboRow({ title: 'Color theme', model });
    ignoreScroll(row);
    const sync = () => row.set_selected(Math.max(0, keys.indexOf(settings.get_string('theme-mode'))));
    sync();
    row.connect('notify::selected', () => settings.set_string('theme-mode', keys[row.get_selected()]));
    settings.connect('changed::theme-mode', sync);
    group.add(row);
}

export default class NotistackPreferences extends ExtensionPreferences {
    fillPreferencesWindow(window) {
        const settings = this.getSettings('org.gnome.shell.extensions.notistack');
        const page = new Adw.PreferencesPage();
        window.add(page);

        // --- Appearance ---
        const appearance = new Adw.PreferencesGroup({ title: 'Appearance' });
        page.add(appearance);
        addThemeCombo(appearance, settings);
        addEntry(appearance, settings, 'background-color', 'Background override (#rrggbb, empty = theme)');
        addEntry(appearance, settings, 'foreground-color', 'Text color override (#rrggbb, empty = theme)');
        addEntry(appearance, settings, 'font-family', 'Font family (empty = system)');
        addSpin(appearance, settings, 'summary-size-pt', 'Title size (pt, 0 = from system font)',
            { lower: 0, upper: 72, step: 1, digits: 0, isDouble: true });
        addSpin(appearance, settings, 'body-size-pt', 'Body size (pt, 0 = from system font)',
            { lower: 0, upper: 72, step: 1, digits: 0, isDouble: true });
        addSpin(appearance, settings, 'title-body-gap-px', 'Gap between title and body (px)',
            { lower: 0, upper: 60, step: 1 });

        // --- Layout ---
        const layout = new Adw.PreferencesGroup({
            title: 'Layout',
            description: 'Width derives from the monitor unless an absolute width is set.',
        });
        page.add(layout);
        addMonitorCombo(layout, settings);
        addSpin(layout, settings, 'width-px', 'Width (px, 0 = automatic)',
            { lower: 0, upper: 4000, step: 10 });
        addSpin(layout, settings, 'width-height-fraction', 'Width as fraction of monitor height',
            { lower: 0.05, upper: 1.0, step: 0.01, digits: 2, isDouble: true });
        addSpin(layout, settings, 'max-width-fraction', 'Max width as fraction of monitor width',
            { lower: 0.05, upper: 1.0, step: 0.01, digits: 2, isDouble: true });
        addSpin(layout, settings, 'gap-px', 'Gap between popups (px)', { lower: 0, upper: 100, step: 1 });
        addSpin(layout, settings, 'margin-px', 'Screen-edge margin (px)', { lower: 0, upper: 200, step: 1 });
        addSpin(layout, settings, 'max-stack', 'Max simultaneous popups', { lower: 1, upper: 20, step: 1 });

        // --- Timing ---
        const timing = new Adw.PreferencesGroup({ title: 'Timing' });
        page.add(timing);
        addSpin(timing, settings, 'default-timeout-ms', 'Default timeout (ms)',
            { lower: 0, upper: 120000, step: 500 });
        addSpin(timing, settings, 'low-urgency-timeout-ms', 'Low-urgency timeout (ms)',
            { lower: 0, upper: 120000, step: 500 });
        addSpin(timing, settings, 'min-timeout-ms', 'Min display time (ms, 0 = per-notification)',
            { lower: 0, upper: 120000, step: 500 });
        addSpin(timing, settings, 'max-timeout-ms', 'Max display time (ms, 0 = per-notification)',
            { lower: 0, upper: 120000, step: 500 });
        addSpin(timing, settings, 'fade-ms', 'Fade duration (ms, 0 = instant)',
            { lower: 0, upper: 2000, step: 10 });

        // --- Behavior ---
        const behavior = new Adw.PreferencesGroup({ title: 'Behavior' });
        page.add(behavior);
        addSpin(behavior, settings, 'history-size', 'History size', { lower: 0, upper: 1000, step: 10 });
        addSwitch(behavior, settings, 'suppress-on-fullscreen', 'Suppress on fullscreen',
            'Hide popups while a fullscreen window is focused');
        addSwitch(behavior, settings, 'gtk-takeover', 'Take over native-GTK notifications',
            'Covers org.gtk.Notifications. Takes effect on the next daemon start.');
    }
}
