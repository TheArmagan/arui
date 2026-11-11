// The mod API. A mod's entry module exports `activate(ctx: AruiContext)`.
//
// Execution model: every surface window (taskbar on each monitor, start menu, flyouts, ...) and
// the hidden `host` window load all enabled mods and call `activate` once per window.
// Registrations are per window; check `ctx.surface.id` if a mod only cares about some surfaces.
// Surface *definitions* are collected from the host window and sent to the shell, which creates
// the actual windows. Mods are fully trusted.

import type { Component } from 'svelte';
import type {
	AppEntry,
	Hwnd,
	MediaAction,
	MediaSession,
	MonitorInfo,
	NetworkState,
	Rect,
	TrayAction,
	TrayIcon,
	VolumeState,
	WindowInfo
} from './types';
import type {
	CommandName,
	Commands,
	InstalledMod,
	ModManifest,
	SurfaceOpenArgs,
	SurfaceWindowOptions
} from './protocol';

export const API_VERSION = 1;

export type Dispose = () => void;

// ---------------------------------------------------------------- state

/** A group of windows sharing an appId (one taskbar button). Pinned apps without windows have no windows. */
export interface AppGroup {
	appId: string;
	name: string;
	iconKey: string;
	exePath: string | null;
	pinned: boolean;
	/** Start-menu entry used to launch this app, if known. */
	app: AppEntry | null;
	windows: WindowInfo[];
	isFocused: boolean;
	isFlashing: boolean;
}

export interface PinnedItem {
	/** AppEntry id, or `exe:<path>` for arbitrary executables. */
	id: string;
	appId: string;
	name: string;
	iconKey: string;
	launch: { kind: 'app'; id: string } | { kind: 'exe'; path: string };
}

/**
 * Reactive (Svelte 5 `$state`) read-only view of the system. Read it inside components or
 * `$derived`/`$effect` and it updates live. Do not mutate; use `ctx.actions`.
 */
export interface AruiState {
	monitors: MonitorInfo[];
	/** All taskbar-eligible windows, z-order not guaranteed. */
	windows: WindowInfo[];
	focusedHwnd: Hwnd | null;
	readonly focused: WindowInfo | null;
	/** Pinned + running apps, in taskbar order. */
	readonly groups: AppGroup[];
	pins: PinnedItem[];
	apps: AppEntry[];
	tray: TrayIcon[];
	media: { sessions: MediaSession[]; current: string | null };
	readonly currentMedia: MediaSession | null;
	volume: VolumeState;
	network: NetworkState;
	/** Updated every second. */
	now: Date;
	mods: InstalledMod[];
}

// ---------------------------------------------------------------- actions

export interface AruiActions {
	focusWindow(hwnd: Hwnd): Promise<void>;
	/** Shows a window in front without activating it (hover peek). One peek at a time. */
	peekWindow(hwnd: Hwnd): Promise<void>;
	/** Ends the peek: `restore` puts the window back where it was, otherwise it stays in front. */
	unpeekWindow(restore?: boolean): Promise<void>;
	toggleWindow(hwnd: Hwnd): Promise<void>;
	minimizeWindow(hwnd: Hwnd): Promise<void>;
	maximizeWindow(hwnd: Hwnd): Promise<void>;
	restoreWindow(hwnd: Hwnd): Promise<void>;
	closeWindow(hwnd: Hwnd): Promise<void>;
	/** Click on a taskbar group: launch if not running, toggle if one window, cycle/preview if many. */
	activateGroup(group: AppGroup): Promise<void>;

	launchApp(id: string): Promise<void>;
	shellExecute(path: string, args?: string, verb?: string): Promise<void>;
	openWindowsStart(): Promise<void>;

	pin(item: PinnedItem): void;
	unpin(id: string): void;
	movePin(id: string, toIndex: number): void;

	trayAction(id: string, action: TrayAction, x: number, y: number): Promise<void>;

	media(action: MediaAction, sessionId?: string): Promise<void>;
	setVolume(level: number): Promise<void>;
	setMuted(muted: boolean): Promise<void>;

	/**
	 * Live window thumbnail drawn into this window at `rect` (CSS px, window-relative),
	 * aspect-fit. Resolves with the source window's size (physical px) for sizing the box.
	 */
	showPreview(hwnd: Hwnd, rect: Rect): Promise<{ width: number; height: number }>;
	hidePreview(hwnd?: Hwnd): Promise<void>;

	quit(): Promise<void>;
	restart(): Promise<void>;
}

// ---------------------------------------------------------------- registries

/** Built-in slot names. Mods may also render their own slots with `<Slot name="mymod.x" />`. */
export type CoreSlot =
	| 'taskbar.start'
	| 'taskbar.left'
	| 'taskbar.center'
	| 'taskbar.right'
	| 'taskbar.end'
	| 'start.header'
	| 'start.pinned'
	| 'start.list'
	| 'start.footer'
	| 'tray.inline'
	| 'tray.flyout'
	| 'quick.settings'
	| 'settings.pages';

export type SlotName = CoreSlot | (string & {});

export interface WidgetOptions<P extends Record<string, unknown> = Record<string, unknown>> {
	/** Unique within the slot; registering the same id again replaces it. Default `<modId>:<n>`. */
	id?: string;
	/** Lower comes first. Core widgets use multiples of 100. */
	order?: number;
	props?: P;
}

export interface WidgetEntry {
	id: string;
	modId: string;
	slot: string;
	order: number;
	component: Component<any>;
	props: Record<string, unknown>;
}

export interface SurfaceDefinition {
	id: string;
	component: Component<any>;
	window: SurfaceWindowOptions;
}

export interface SurfaceInfo {
	/** Surface rendered by this window ('host' for the hidden host window). */
	id: string;
	/** Monitor id for per-monitor surfaces. */
	monitor: string | null;
	/** Params passed to surfaceOpen. Reactive. */
	readonly params: Record<string, unknown>;
	/** Reactive: true while the window is shown. */
	readonly visible: boolean;
	resize(size: { width?: number; height?: number; thickness?: number }): Promise<void>;
	close(): Promise<void>;
}

export interface ThemeDefinition {
	id: string;
	name: string;
	/** CSS text (use :root { --arui-*: ... }). */
	css?: string;
	/** Or a URL to a stylesheet (ctx.asset('theme.css')). */
	url?: string;
	/** Or token overrides without the `--arui-` prefix, e.g. { bg: '#000', radius: '12px' }. */
	tokens?: Record<string, string>;
}

export type SettingType = 'boolean' | 'number' | 'string' | 'select' | 'color';

export interface SettingField {
	key: string;
	label: string;
	type: SettingType;
	default: unknown;
	description?: string;
	/** select */
	options?: { value: string; label: string }[];
	/** number */
	min?: number;
	max?: number;
	step?: number;
}

export interface SettingsSchema {
	title?: string;
	fields: SettingField[];
}

export interface MenuItem {
	id: string;
	label: string;
	/** Lucide icon component or icon URL. */
	icon?: Component<any> | string;
	separatorBefore?: boolean;
	disabled?: boolean;
	onSelect: () => void;
	children?: MenuItem[];
}

/** Context menu targets and what the item builder receives. */
export interface ContextMenuTargets {
	taskbar: { monitor: string | null };
	'taskbar.group': { group: AppGroup };
	'start.app': { app: AppEntry };
	'tray.icon': { icon: TrayIcon };
}

export type ContextMenuTarget = keyof ContextMenuTargets;

// ---------------------------------------------------------------- context

export interface AruiContext {
	readonly mod: ModManifest;
	readonly apiVersion: number;
	readonly state: AruiState;
	readonly actions: AruiActions;
	readonly surface: SurfaceInfo;

	widgets: {
		register<P extends Record<string, unknown>>(
			slot: SlotName,
			component: Component<P>,
			opts?: WidgetOptions<P>
		): Dispose;
		/** Hide a widget registered by another mod (e.g. 'core.media:card'). */
		hide(widgetId: string): Dispose;
	};

	surfaces: {
		/** Define a new surface (or replace an existing one with the same id, e.g. 'taskbar'). */
		register(def: SurfaceDefinition): Dispose;
		open(args: SurfaceOpenArgs | string): Promise<void>;
		close(id: string, monitor?: string): Promise<void>;
		toggle(args: SurfaceOpenArgs | string): Promise<void>;
	};

	themes: {
		register(theme: ThemeDefinition): Dispose;
		/** Activate by id (persisted in settings). */
		activate(id: string): void;
		readonly active: string;
	};

	commands: {
		register(id: string, handler: (...args: any[]) => unknown): Dispose;
		run(id: string, ...args: any[]): Promise<unknown>;
	};

	/** Cross-window events (every surface of every mod receives them). */
	events: {
		on<T = unknown>(name: string, handler: (payload: T, from: string) => void): Dispose;
		emit(name: string, payload?: unknown): Promise<void>;
	};

	/** Per-mod settings namespace (the mod id). Values are reactive. */
	settings: {
		define(schema: SettingsSchema): void;
		get<T = unknown>(key: string): T;
		set(key: string, value: unknown): Promise<void>;
		/** Read another namespace, e.g. ('core', 'theme'). */
		read<T = unknown>(namespace: string, key: string): T;
	};

	contextMenus: {
		extend<T extends ContextMenuTarget>(
			target: T,
			build: (target: ContextMenuTargets[T]) => MenuItem[]
		): Dispose;
	};

	/** Absolute URL of a file in the mod folder. */
	asset(path: string): string;
	/** Icon-store URL for an icon key. */
	iconUrl(key: string | null | undefined): string;
	/** Raw Tauri command access. */
	invoke<C extends CommandName>(
		cmd: C,
		...args: Commands[C]['args'] extends void ? [] : [Commands[C]['args']]
	): Promise<Commands[C]['result']>;
	log: {
		info(...args: unknown[]): void;
		warn(...args: unknown[]): void;
		error(...args: unknown[]): void;
	};
	/** Run on deactivate (hot reload, disable, window close). */
	onDispose(fn: Dispose): void;
}

export interface ModModule {
	activate(ctx: AruiContext): void | Dispose | Promise<void | Dispose>;
}
