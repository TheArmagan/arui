// Built-in mods, in activation order. They use exactly the same ModModule/ctx API as external
// mods; external mods are activated after these, so their registrations win on equal ids.

import type { BuiltinMod } from '../lib/runtime/runtime.svelte';
import * as clock from './core.clock';
import * as media from './core.media';
import * as menus from './core.menus';
import * as previews from './core.previews';
import * as quick from './core.quick-settings';
import * as settings from './core.settings';
import * as startMenu from './core.start-menu';
import * as taskbar from './core.taskbar';
import * as theme from './core.theme-default';
import * as tray from './core.tray';

export const BUILTINS: BuiltinMod[] = [
	{ manifest: theme.manifest, module: theme.default, required: true },
	{ manifest: menus.manifest, module: menus.default, required: true },
	{ manifest: settings.manifest, module: settings.default, required: true },
	{ manifest: taskbar.manifest, module: taskbar.default },
	{ manifest: startMenu.manifest, module: startMenu.default },
	{ manifest: tray.manifest, module: tray.default },
	{ manifest: quick.manifest, module: quick.default },
	{ manifest: media.manifest, module: media.default },
	{ manifest: clock.manifest, module: clock.default },
	{ manifest: previews.manifest, module: previews.default }
];
