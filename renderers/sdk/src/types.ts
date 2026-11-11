// Data contract between natives and the UI.
// Mirrors natives/arui-win/src/types.rs. Keep both files in sync.
// Enums are adjacently tagged: { type: "added", data: {...} }.

export type Hwnd = number;

export interface Rect {
	left: number;
	top: number;
	right: number;
	bottom: number;
}

export type Edge = 'left' | 'top' | 'right' | 'bottom';

// ---------------------------------------------------------------- monitors

export interface MonitorInfo {
	/** Stable id: GDI device name, e.g. `\\.\DISPLAY1`. */
	id: string;
	handle: number;
	/** Physical pixels. */
	bounds: Rect;
	workArea: Rect;
	/** 1.0 = 96 DPI. */
	scale: number;
	primary: boolean;
}

// ---------------------------------------------------------------- windows

export interface WindowInfo {
	hwnd: Hwnd;
	pid: number;
	title: string;
	className: string;
	exePath: string;
	/** Grouping key: AppUserModelID, or lowercased exe path. */
	appId: string;
	/** Use with `iconUrl(key)`. */
	iconKey: string;
	monitor: number;
	isMinimized: boolean;
	isMaximized: boolean;
	isFocused: boolean;
	isFlashing: boolean;
}

export type WindowEvent =
	| { type: 'snapshot'; data: WindowInfo[] }
	| { type: 'added'; data: WindowInfo }
	| { type: 'updated'; data: WindowInfo }
	| { type: 'removed'; data: { hwnd: Hwnd } }
	| { type: 'focused'; data: { hwnd: Hwnd | null } };

// ---------------------------------------------------------------- start menu apps

export type AppKind = 'shortcut' | 'packaged';

export interface AppEntry {
	id: string;
	name: string;
	kind: AppKind;
	target: string;
	exePath: string | null;
	appId: string | null;
	iconKey: string;
	folder: string | null;
}

// ---------------------------------------------------------------- tray

export interface TrayIcon {
	id: string;
	ownerHwnd: Hwnd;
	uid: number;
	guid: string | null;
	callbackMessage: number;
	version: number;
	tooltip: string;
	iconKey: string;
	visible: boolean;
	exePath: string | null;
}

export type TrayEvent =
	| { type: 'snapshot'; data: TrayIcon[] }
	| { type: 'added'; data: TrayIcon }
	| { type: 'updated'; data: TrayIcon }
	| { type: 'removed'; data: { id: string } }
	| { type: 'balloon'; data: { id: string; title: string; text: string } };

export type TrayAction =
	| 'leftClick'
	| 'rightClick'
	| 'middleClick'
	| 'doubleClick'
	| 'hoverEnter'
	| 'hoverLeave';

// ---------------------------------------------------------------- media

export type PlaybackStatus = 'closed' | 'opened' | 'changing' | 'stopped' | 'playing' | 'paused';

export interface MediaSession {
	id: string;
	appId: string;
	title: string;
	artist: string;
	album: string;
	status: PlaybackStatus;
	canPlay: boolean;
	canPause: boolean;
	canNext: boolean;
	canPrevious: boolean;
	positionMs: number | null;
	durationMs: number | null;
	/** Unix ms when positionMs was sampled. */
	positionAtMs: number | null;
	artworkKey: string | null;
}

export type MediaEvent =
	| { type: 'snapshot'; data: { sessions: MediaSession[]; current: string | null } }
	| { type: 'updated'; data: MediaSession }
	| { type: 'removed'; data: { id: string } }
	| { type: 'current'; data: { id: string | null } };

export type MediaAction = 'play' | 'pause' | 'togglePlayPause' | 'next' | 'previous' | 'stop';

// ---------------------------------------------------------------- system

export interface VolumeState {
	/** 0..1 */
	level: number;
	muted: boolean;
}

export type NetworkKind = 'none' | 'ethernet' | 'wifi' | 'cellular' | 'other';

export interface NetworkState {
	connected: boolean;
	internet: boolean;
	kind: NetworkKind;
	name: string | null;
}

export type SystemEvent =
	| { type: 'volume'; data: VolumeState }
	| { type: 'network'; data: NetworkState };

// ---------------------------------------------------------------- shell

export type ShellEvent =
	| { type: 'monitorsChanged'; data: MonitorInfo[] }
	| { type: 'explorerRestarted' }
	| { type: 'startKey' }
	| { type: 'appsChanged'; data: AppEntry[] }
	| { type: 'fullscreen'; data: { monitor: number; active: boolean } };
