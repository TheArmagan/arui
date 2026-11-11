// AruiActions: thin wrappers over host commands plus pin management.

import type { AppGroup, AruiActions, PinnedItem } from '@arui/sdk';
import type { Backend } from '../backend';
import type { Store } from './state.svelte';

export function createActions(backend: Backend, store: Store): AruiActions {
	const inv = backend.invoke.bind(backend);

	const launchGroup = async (group: AppGroup) => {
		const pin = store.pins.find((p) => p.appId.toLowerCase() === group.appId.toLowerCase());
		if (pin?.launch.kind === 'app') return inv('app_launch', { id: pin.launch.id });
		if (pin?.launch.kind === 'exe') return inv('shell_execute', { path: pin.launch.path });
		if (group.app) return inv('app_launch', { id: group.app.id });
		if (group.exePath) return inv('shell_execute', { path: group.exePath });
	};

	const actions: AruiActions = {
		focusWindow: (hwnd) => inv('window_focus', { hwnd }),
		toggleWindow: (hwnd) => inv('window_toggle', { hwnd }),
		minimizeWindow: (hwnd) => inv('window_minimize', { hwnd }),
		maximizeWindow: (hwnd) => inv('window_maximize', { hwnd }),
		restoreWindow: (hwnd) => inv('window_restore', { hwnd }),
		closeWindow: (hwnd) => inv('window_close', { hwnd }),

		async activateGroup(group) {
			const wins = group.windows;
			if (wins.length === 0) return launchGroup(group);
			if (wins.length === 1) return inv('window_toggle', { hwnd: wins[0].hwnd });
			// Several windows: cycle focus through them in taskbar order.
			const i = wins.findIndex((w) => w.hwnd === store.focusedHwnd);
			const next = wins[(i + 1) % wins.length];
			return inv('window_focus', { hwnd: next.hwnd });
		},

		launchApp: (id) => inv('app_launch', { id }),
		shellExecute: (path, args, verb) => inv('shell_execute', { path, args, verb }),
		openWindowsStart: () => inv('shell_open_windows_start'),

		pin(item: PinnedItem) {
			if (store.pins.some((p) => p.id === item.id || p.appId.toLowerCase() === item.appId.toLowerCase())) return;
			store.pins = [...store.pins, item];
		},
		unpin(id) {
			store.pins = store.pins.filter((p) => p.id !== id && p.appId !== id);
		},
		movePin(id, toIndex) {
			const list = [...store.pins];
			const from = list.findIndex((p) => p.id === id);
			if (from < 0) return;
			const [item] = list.splice(from, 1);
			list.splice(Math.max(0, Math.min(toIndex, list.length)), 0, item);
			store.pins = list;
		},

		trayAction: (id, action, x, y) => inv('tray_action', { id, action, x, y }),

		media: (action, sessionId) => inv('media_control', { id: sessionId, action }),
		setVolume: (level) => inv('volume_set', { level: Math.max(0, Math.min(1, level)) }),
		setMuted: (muted) => inv('volume_mute', { muted }),

		showPreview: (hwnd, rect) => inv('preview_show', { hwnd, rect }),
		hidePreview: (hwnd) => inv('preview_hide', { hwnd }),

		quit: () => inv('app_quit'),
		restart: () => inv('app_restart')
	};
	return actions;
}
