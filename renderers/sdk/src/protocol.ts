// Host <-> UI protocol: every Tauri command and event the shell app exposes.
// The Rust side lives in renderers/shell/src-tauri. Tauri converts camelCase argument
// names to snake_case Rust parameters automatically.

import type {
	AppEntry,
	Edge,
	Hwnd,
	MediaAction,
	MediaEvent,
	MediaSession,
	MonitorInfo,
	NetworkState,
	Rect,
	ShellEvent,
	SystemEvent,
	TrayAction,
	TrayEvent,
	TrayIcon,
	VolumeState,
	WindowEvent,
	WindowInfo
} from './types';

// ---------------------------------------------------------------- surfaces

export type SurfaceAnchor =
	| 'center'
	| 'cursor'
	/** Positioned next to the calling surface's element rect (pass `rect` to surfaceOpen). */
	| 'rect'
	| 'bottomLeft'
	| 'bottomCenter'
	| 'bottomRight'
	| 'topLeft'
	| 'topCenter'
	| 'topRight';

export interface SurfaceWindowOptions {
	/**
	 * appbar: docked to a monitor edge, reserves work area (taskbar, docks).
	 * popup:  transient, closes on blur by default (start menu, flyouts, previews).
	 * panel:  free floating window (widgets, settings).
	 */
	kind: 'appbar' | 'popup' | 'panel';
	/** appbar: edge to dock to. Default 'bottom'. */
	edge?: Edge;
	/** appbar: thickness in DIP (height for top/bottom). */
	thickness?: number;
	/** appbar: whether to reserve work area (false = overlay, e.g. auto-hide). Default true. */
	reserve?: boolean;
	/** appbar: one window per monitor. Default true. */
	perMonitor?: boolean;
	/** popup/panel size in DIP. */
	width?: number;
	height?: number;
	anchor?: SurfaceAnchor;
	/** Gap in DIP between anchor and window. */
	offset?: number;
	/** popup: default true. */
	closeOnBlur?: boolean;
	/** Whether the window can take keyboard focus. appbar default false, others true. */
	focusable?: boolean;
	/** Open automatically at startup. appbar default true, others false. */
	autoOpen?: boolean;
}

/** Sent to the host by the `host` window after mods register surfaces. */
export interface SurfaceSpec {
	id: string;
	/** Mod that currently provides this surface. */
	modId: string;
	window: SurfaceWindowOptions;
}

export interface SurfaceOpenArgs {
	id: string;
	/** Monitor id; default: monitor under cursor. */
	monitor?: string;
	/** Screen rect in physical px for anchor 'rect'. */
	rect?: Rect;
	/** Arbitrary data passed to the surface (readable via ctx.surface.params). */
	params?: Record<string, unknown>;
}

// ---------------------------------------------------------------- mods & settings

export interface ModManifest {
	id: string;
	name: string;
	version: string;
	/** SDK major the mod was built for. */
	apiVersion: number;
	description?: string;
	author?: string;
	/** JS entry relative to the mod folder. Default "dist/index.js". */
	entry?: string;
	/** CSS files injected into every surface while the mod is enabled. */
	styles?: string[];
}

export interface InstalledMod {
	manifest: ModManifest;
	/** Base URL to load files from, e.g. `arui-mod://localhost/<id>/` or `/builtin/<id>/`. */
	baseUrl: string;
	builtin: boolean;
	enabled: boolean;
}

/** settings.json: per-namespace objects. Core uses "core", mods use their id. */
export type SettingsFile = Record<string, Record<string, unknown>>;

// ---------------------------------------------------------------- commands

export interface Commands {
	// windows
	windows_list: { args: void; result: WindowInfo[] };
	window_focus: { args: { hwnd: Hwnd }; result: void };
	/** Focus if not focused, minimize if focused. */
	window_toggle: { args: { hwnd: Hwnd }; result: void };
	window_minimize: { args: { hwnd: Hwnd }; result: void };
	window_maximize: { args: { hwnd: Hwnd }; result: void };
	window_restore: { args: { hwnd: Hwnd }; result: void };
	window_close: { args: { hwnd: Hwnd }; result: void };
	/** Live DWM thumbnail of `hwnd` drawn into the calling window at `rect` (CSS px, relative to the window). */
	preview_show: { args: { hwnd: Hwnd; rect: Rect }; result: void };
	preview_hide: { args: { hwnd?: Hwnd }; result: void };

	// apps / launching
	apps_list: { args: void; result: AppEntry[] };
	app_launch: { args: { id: string }; result: void };
	/** Run an executable, file, URL or shell: path. */
	shell_execute: { args: { path: string; args?: string; verb?: string }; result: void };
	/** Open the original Windows start menu. */
	shell_open_windows_start: { args: void; result: void };

	// tray
	tray_list: { args: void; result: TrayIcon[] };
	/** x/y: screen px where the click happened (apps position their menus there). */
	tray_action: { args: { id: string; action: TrayAction; x: number; y: number }; result: void };

	// media
	media_list: { args: void; result: { sessions: MediaSession[]; current: string | null } };
	/** id omitted = current session. */
	media_control: { args: { id?: string; action: MediaAction }; result: void };

	// system
	volume_get: { args: void; result: VolumeState };
	volume_set: { args: { level: number }; result: void };
	volume_mute: { args: { muted: boolean }; result: void };
	network_get: { args: void; result: NetworkState };

	// shell
	monitors_list: { args: void; result: MonitorInfo[] };

	// surfaces
	/** Replace the full set of surface specs (host window only). */
	surfaces_define: { args: { specs: SurfaceSpec[] }; result: void };
	surface_open: { args: SurfaceOpenArgs; result: void };
	surface_close: { args: { id: string; monitor?: string }; result: void };
	surface_toggle: { args: SurfaceOpenArgs; result: void };
	/** Resize the calling surface window (DIP). Appbars resize their reserved area too. */
	surface_resize: { args: { width?: number; height?: number; thickness?: number }; result: void };
	/** Params the calling surface window was opened with. */
	surface_params: { args: void; result: Record<string, unknown> };

	// mods & settings
	mods_list: { args: void; result: InstalledMod[] };
	mod_set_enabled: { args: { id: string; enabled: boolean }; result: void };
	mods_open_folder: { args: void; result: void };
	settings_get: { args: void; result: SettingsFile };
	settings_set: { args: { namespace: string; key: string; value: unknown }; result: void };

	// app
	app_quit: { args: void; result: void };
	app_restart: { args: void; result: void };
}

export type CommandName = keyof Commands;

// ---------------------------------------------------------------- events

export interface Events {
	'native://window': WindowEvent;
	'native://tray': TrayEvent;
	'native://media': MediaEvent;
	'native://system': SystemEvent;
	'native://shell': ShellEvent;
	/** A file in a mod folder changed (hot reload). */
	'arui://mod-changed': { id: string };
	/** Mod list or enabled state changed. */
	'arui://mods-changed': InstalledMod[];
	'arui://settings-changed': { namespace: string; key: string; value: unknown };
	/** Surface window was shown (payload: params) / hidden. Sent only to that window. */
	'arui://surface-shown': { params: Record<string, unknown> };
	'arui://surface-hidden': Record<string, never>;
	/** Cross-surface mod/user events: ctx.events.emit(). */
	'arui://broadcast': { name: string; payload: unknown; from: string };
}

export type EventName = keyof Events;

/** Custom URI schemes served by the host. */
export const ICON_SCHEME = 'arui-icon';
export const MOD_SCHEME = 'arui-mod';

/** URL for an icon-store key. */
export function iconUrl(key: string | null | undefined): string {
	if (!key) return '';
	// WebView2 on Windows exposes custom schemes as http://<scheme>.localhost/.
	return `http://${ICON_SCHEME}.localhost/${encodeURIComponent(key)}`;
}
