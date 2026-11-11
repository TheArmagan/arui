// Real host backend: Tauri 2 commands and events.

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { emit as tauriEmit, listen as tauriListen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { iconUrl as sdkIconUrl, type CommandName, type EventName, type Events } from '@arui/sdk';
import type { ArgsOf, Backend, Unlisten } from './types';

// Events the host sends to one window (emit_to). A plain listen() uses the `Any` target and
// would also receive other windows' copies, e.g. every popup thinking it was shown.
const WINDOW_EVENTS = new Set<string>(['arui://surface-shown', 'arui://surface-hidden']);

export function createTauriBackend(label: string): Backend {
	const own = getCurrentWebviewWindow().label;
	return {
		kind: 'tauri',
		label,
		invoke<C extends CommandName>(cmd: C, ...args: ArgsOf<C>) {
			return tauriInvoke(cmd, (args[0] ?? {}) as Record<string, unknown>);
		},
		async listen<E extends EventName>(event: E, cb: (payload: Events[E]) => void): Promise<Unlisten> {
			const target = WINDOW_EVENTS.has(event) ? { kind: 'AnyLabel' as const, label: own } : undefined;
			return tauriListen<Events[E]>(event, (e) => cb(e.payload), target ? { target } : undefined);
		},
		broadcast(payload) {
			// Tauri's global emit reaches every webview, including the sender.
			return tauriEmit('arui://broadcast', payload);
		},
		iconUrl: sdkIconUrl,
		screenRect(r) {
			const s = window.devicePixelRatio || 1;
			return {
				left: Math.round((window.screenX + r.left) * s),
				top: Math.round((window.screenY + r.top) * s),
				right: Math.round((window.screenX + r.right) * s),
				bottom: Math.round((window.screenY + r.bottom) * s)
			};
		}
	};
}
