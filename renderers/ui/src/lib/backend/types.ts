// Backend abstraction: the UI talks to the host only through this interface, typed with the
// protocol in @arui/sdk. Implementations: Tauri (real host) and the in-browser mock.

import type { CommandName, Commands, EventName, Events, Rect } from '@arui/sdk';

export type Unlisten = () => void;

export type ArgsOf<C extends CommandName> = Commands[C]['args'] extends void
	? []
	: [Commands[C]['args']];

export interface Backend {
	readonly kind: 'tauri' | 'mock';
	/** Label of this window, e.g. `taskbar:\\.\DISPLAY1`, `start-menu`, `host`. */
	readonly label: string;
	invoke<C extends CommandName>(cmd: C, ...args: ArgsOf<C>): Promise<Commands[C]['result']>;
	listen<E extends EventName>(event: E, cb: (payload: Events[E]) => void): Promise<Unlisten>;
	/** Send `arui://broadcast` to every window (including this one). */
	broadcast(payload: Events['arui://broadcast']): Promise<void>;
	/** URL for an icon-store key (real icon scheme or generated mock placeholders). */
	iconUrl(key: string | null | undefined): string;
	/** Convert a rect in this window's CSS px (getBoundingClientRect) to screen physical px. */
	screenRect(r: { left: number; top: number; right: number; bottom: number }): Rect;
}

export interface SurfaceLocation {
	surface: string;
	monitor: string | null;
}

export function readLocation(): SurfaceLocation {
	const params = new URLSearchParams(location.search);
	return {
		surface: params.get('surface') || 'dev',
		monitor: params.get('monitor')
	};
}

export function labelFor(loc: SurfaceLocation): string {
	return loc.monitor ? `${loc.surface}:${loc.monitor}` : loc.surface;
}
