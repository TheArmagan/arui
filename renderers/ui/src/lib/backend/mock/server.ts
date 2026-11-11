// In-browser mock of the ARUI host. It implements every command and event of the protocol
// against fake state, so the UI (and mods) can run in a normal browser.
//
// One server instance lives in the dev page (`?surface=dev`); every surface iframe connects to
// it through `window.parent.__ARUI_MOCK_SERVER__`, so all windows share one consistent fake
// system, exactly like real windows share the Rust host. A surface opened standalone (no dev
// parent) creates its own private server.

import type {
	AppEntry,
	CommandName,
	Commands,
	EventName,
	Events,
	InstalledMod,
	MediaSession,
	ModManifest,
	MonitorInfo,
	NetworkState,
	Rect,
	SettingsFile,
	SurfaceSpec,
	TrayIcon,
	VolumeState,
	WindowInfo
} from '@arui/sdk';
import {
	TRACKS,
	seedApps,
	seedMonitors,
	seedTray,
	seedWindows,
	sessionFor,
	windowFor
} from './data';

const SETTINGS_KEY = 'arui.mock.settings.v1';
const MODS_KEY = 'arui.mock.mods.v1';

/** Mods enabled the first time the mock runs. */
const DEFAULT_ENABLED = new Set(['focus-timer']);

function load<T>(key: string, fallback: T): T {
	try {
		const raw = localStorage.getItem(key);
		return raw ? (JSON.parse(raw) as T) : fallback;
	} catch {
		return fallback;
	}
}

function save(key: string, value: unknown) {
	try {
		localStorage.setItem(key, JSON.stringify(value));
	} catch {
		// Storage can be unavailable (private mode); the mock then just forgets on reload.
	}
}

export interface Frame {
	x: number;
	y: number;
	width: number;
	height: number;
}

export interface SurfaceInstance {
	/** Window label, also the key: `<id>` or `<id>:<monitor>` for per-monitor appbars. */
	label: string;
	id: string;
	monitor: string | null;
	spec: SurfaceSpec;
	visible: boolean;
	params: Record<string, unknown>;
	rect: Rect | null;
	size: { width?: number; height?: number; thickness?: number };
	frame: Frame;
}

export interface MockLogEntry {
	at: number;
	from: string;
	cmd: string;
	args: unknown;
}

type Handler = (payload: unknown) => void;

export interface MockConnection {
	readonly label: string;
	invoke(cmd: CommandName, args: unknown): Promise<unknown>;
	on(event: EventName, cb: Handler): () => void;
	close(): void;
}

interface ClientRecord {
	label: string;
	handlers: Map<string, Set<Handler>>;
}

type CommandImpl = (args: any, from: string) => unknown | Promise<unknown>;

function clone<T>(v: T): T {
	return v === undefined ? v : structuredClone(v);
}

export class MockServer {
	monitors: MonitorInfo[];
	windows: WindowInfo[];
	apps: AppEntry[] = seedApps();
	tray: TrayIcon[] = seedTray();
	media: { sessions: MediaSession[]; current: string | null };
	volume: VolumeState = { level: 0.42, muted: false };
	network: NetworkState = { connected: true, internet: true, kind: 'wifi', name: 'Moda-5G' };
	settings: SettingsFile = load<SettingsFile>(SETTINGS_KEY, {});
	mods: InstalledMod[] = [];
	specs: SurfaceSpec[] = [];
	instances = new Map<string, SurfaceInstance>();
	log: MockLogEntry[] = [];

	private clients = new Set<ClientRecord>();
	private devListeners = new Set<() => void>();
	private trackIndex = 0;
	private blurClosedAt = new Map<string, number>();
	private modsReady: Promise<void>;
	private enabled: Record<string, boolean> = load(MODS_KEY, {});

	constructor(width = window.innerWidth, height = window.innerHeight) {
		this.monitors = seedMonitors(width, height);
		this.windows = seedWindows(this.apps);
		this.media = { sessions: [sessionFor(0, 'playing', 48_000)], current: 'Spotify.exe' };
		if (!this.settings.core?.pins) {
			// First run: a few taskbar pins, like a fresh Windows profile.
			const pin = (key: string) => {
				const a = this.apps.find((x) => x.id === `app:${key}`)!;
				return {
					id: a.id,
					appId: a.appId ?? a.id,
					name: a.name,
					iconKey: a.iconKey,
					launch: { kind: 'app', id: a.id }
				};
			};
			this.settings.core = {
				...(this.settings.core ?? {}),
				pins: ['explorer', 'firefox', 'code', 'terminal', 'steam'].map(pin)
			};
		}
		this.modsReady = this.refreshMods(false);
		this.startSimulation();
		if (import.meta.hot) {
			import.meta.hot.on('arui:mod-changed', async (data: { id: string }) => {
				await this.refreshMods(true);
				this.emit('arui://mod-changed', { id: data.id });
				this.record('host', 'event:arui://mod-changed', data);
			});
		}
	}

	// ------------------------------------------------------------ clients and events

	connect(label: string): MockConnection {
		const record: ClientRecord = { label, handlers: new Map() };
		this.clients.add(record);
		return {
			label,
			invoke: (cmd, args) => this.invoke(cmd, args, label),
			on: (event, cb) => {
				let set = record.handlers.get(event);
				if (!set) record.handlers.set(event, (set = new Set()));
				set.add(cb);
				return () => set!.delete(cb);
			},
			close: () => {
				this.clients.delete(record);
			}
		};
	}

	emit<E extends EventName>(event: E, payload: Events[E], target?: string) {
		if (event === 'native://window') queueMicrotask(() => this.notify());
		for (const client of [...this.clients]) {
			if (target && client.label !== target) continue;
			const set = client.handlers.get(event);
			if (!set) continue;
			for (const cb of [...set]) {
				try {
					cb(payload);
				} catch (err) {
					// A removed iframe leaves dead handlers behind; drop that client.
					if (String(err).includes('dead object') || String(err).includes('detached')) {
						this.clients.delete(client);
					} else {
						console.error('[mock] event handler failed', event, err);
					}
				}
			}
		}
	}

	/** Dev page subscription: called whenever surfaces, log or monitors change. */
	subscribe(fn: () => void): () => void {
		this.devListeners.add(fn);
		return () => this.devListeners.delete(fn);
	}

	private notify() {
		for (const fn of this.devListeners) fn();
	}

	private record(from: string, cmd: string, args: unknown) {
		this.log.unshift({ at: Date.now(), from, cmd, args });
		if (this.log.length > 80) this.log.length = 80;
		this.notify();
	}

	// ------------------------------------------------------------ commands

	async invoke(cmd: CommandName, args: unknown, from: string): Promise<unknown> {
		const impl = this.commands[cmd];
		if (!impl) throw new Error(`[mock] unknown command ${cmd}`);
		const hover = cmd === 'tray_action' && String((args as { action?: string })?.action).startsWith('hover');
		if (!QUIET.has(cmd) && !hover) this.record(from, cmd, args);
		return clone(await impl(clone(args ?? {}), from));
	}

	private commands: { [C in CommandName]: CommandImpl } = {
		windows_list: () => this.windows,
		window_focus: ({ hwnd }) => this.focus(hwnd),
		window_toggle: ({ hwnd }) => {
			const w = this.win(hwnd);
			if (w?.isFocused && !w.isMinimized) this.minimize(hwnd);
			else this.focus(hwnd);
		},
		window_minimize: ({ hwnd }) => this.minimize(hwnd),
		window_maximize: ({ hwnd }) => this.patchWindow(hwnd, { isMaximized: true, isMinimized: false }),
		window_restore: ({ hwnd }) => this.patchWindow(hwnd, { isMaximized: false, isMinimized: false }),
		window_close: ({ hwnd }) => this.closeWindow(hwnd),
		preview_show: () => undefined,
		preview_hide: () => undefined,

		apps_list: () => this.apps,
		app_launch: ({ id }) => {
			const app = this.apps.find((a) => a.id === id);
			if (!app) throw new Error(`unknown app ${id}`);
			this.openWindow(app, app.name);
		},
		shell_execute: ({ path }) => {
			const base = String(path).split(/[\\/]/).pop()!.toLowerCase();
			const app = this.apps.find(
				(a) => a.exePath && a.exePath.split('\\').pop()!.toLowerCase() === base
			);
			if (app) this.openWindow(app, app.name);
			else if (!/^(https?:|ms-settings:|shell:)/.test(path)) {
				this.openWindow(
					{
						id: `exe:${path}`,
						name: base,
						kind: 'shortcut',
						target: path,
						exePath: path,
						appId: String(path).toLowerCase(),
						iconKey: `mock:app/${base}`,
						folder: null
					},
					base
				);
			}
		},
		shell_open_windows_start: () => undefined,

		tray_list: () => this.tray,
		tray_action: ({ id, action }) => {
			if (action === 'leftClick' && id === 'tray:discord') {
				const w = this.windows.find((x) => x.appId.includes('discord'));
				if (w) this.focus(w.hwnd);
			}
		},

		media_list: () => this.media,
		media_control: ({ action }) => this.mediaControl(action),

		volume_get: () => this.volume,
		volume_set: ({ level }) => {
			this.volume = { ...this.volume, level: Math.max(0, Math.min(1, level)) };
			this.emit('native://system', { type: 'volume', data: clone(this.volume) });
		},
		volume_mute: ({ muted }) => {
			this.volume = { ...this.volume, muted };
			this.emit('native://system', { type: 'volume', data: clone(this.volume) });
		},
		network_get: () => this.network,

		monitors_list: () => this.monitors,

		surfaces_define: ({ specs }) => this.defineSurfaces(specs),
		surface_open: (args) => this.openSurface(args),
		surface_close: ({ id, monitor }) => this.closeSurface(id, monitor),
		surface_toggle: (args) => {
			const inst = this.instanceFor(args.id, args.monitor);
			if (inst?.visible) return this.closeSurface(args.id, args.monitor);
			// A click on the toggle button first blurs (and closes) the popup; do not reopen it.
			if (Date.now() - (this.blurClosedAt.get(args.id) ?? 0) < 300) return;
			return this.openSurface(args);
		},
		surface_resize: (size, from) => {
			const inst = this.instances.get(from);
			if (!inst) return;
			inst.size = { ...inst.size, ...size };
			this.layout();
		},
		surface_params: (_args, from) => this.instances.get(from)?.params ?? {},
		surface_visible: (_args, from) => this.instances.get(from)?.visible ?? true,

		mods_list: async () => {
			await this.modsReady;
			return this.mods;
		},
		mod_set_enabled: ({ id, enabled }) => {
			this.enabled[id] = enabled;
			save(MODS_KEY, this.enabled);
			this.mods = this.mods.map((m) => (m.manifest.id === id ? { ...m, enabled } : m));
			this.emit('arui://mods-changed', clone(this.mods));
		},
		mods_open_folder: () => undefined,
		settings_get: () => this.settings,
		settings_set: ({ namespace, key, value }) => {
			this.settings[namespace] = { ...(this.settings[namespace] ?? {}), [key]: value };
			save(SETTINGS_KEY, this.settings);
			this.emit('arui://settings-changed', clone({ namespace, key, value }));
		},

		app_quit: () => undefined,
		app_restart: () => {
			setTimeout(() => location.reload(), 50);
		}
	};

	// ------------------------------------------------------------ windows

	private win(hwnd: number) {
		return this.windows.find((w) => w.hwnd === hwnd);
	}

	private patchWindow(hwnd: number, patch: Partial<WindowInfo>) {
		const i = this.windows.findIndex((w) => w.hwnd === hwnd);
		if (i < 0) return;
		this.windows[i] = { ...this.windows[i], ...patch };
		this.emit('native://window', { type: 'updated', data: clone(this.windows[i]) });
	}

	focus(hwnd: number | null) {
		for (const w of this.windows) {
			const should = w.hwnd === hwnd;
			if (w.isFocused !== should || (should && w.isMinimized)) {
				this.patchWindow(w.hwnd, {
					isFocused: should,
					isMinimized: should ? false : w.isMinimized,
					isFlashing: should ? false : w.isFlashing
				});
			}
		}
		this.emit('native://window', { type: 'focused', data: { hwnd } });
	}

	private minimize(hwnd: number) {
		this.patchWindow(hwnd, { isMinimized: true, isFocused: false });
		this.emit('native://window', { type: 'focused', data: { hwnd: null } });
	}

	private closeWindow(hwnd: number) {
		const w = this.win(hwnd);
		if (!w) return;
		this.windows = this.windows.filter((x) => x.hwnd !== hwnd);
		this.emit('native://window', { type: 'removed', data: { hwnd } });
		if (w.isFocused) this.emit('native://window', { type: 'focused', data: { hwnd: null } });
	}

	private openWindow(app: AppEntry, title: string) {
		const w = windowFor(app, title);
		this.windows = [...this.windows, w];
		this.emit('native://window', { type: 'added', data: clone(w) });
		this.focus(w.hwnd);
	}

	// ------------------------------------------------------------ media

	private setSession(session: MediaSession) {
		this.media = { sessions: [session], current: session.id };
		this.emit('native://media', { type: 'updated', data: clone(session) });
	}

	private livePosition(s: MediaSession): number {
		if (s.positionMs == null) return 0;
		if (s.status !== 'playing' || s.positionAtMs == null) return s.positionMs;
		return s.positionMs + (Date.now() - s.positionAtMs);
	}

	private mediaControl(action: string): void {
		const s = this.media.sessions[0];
		if (!s) return;
		const pos = this.livePosition(s);
		switch (action) {
			case 'play':
				return this.setSession({ ...s, status: 'playing', positionMs: pos, positionAtMs: Date.now() });
			case 'pause':
				return this.setSession({ ...s, status: 'paused', positionMs: pos, positionAtMs: Date.now() });
			case 'stop':
				return this.setSession({ ...s, status: 'stopped', positionMs: 0, positionAtMs: Date.now() });
			case 'togglePlayPause':
				return this.mediaControl(s.status === 'playing' ? 'pause' : 'play');
			case 'next':
				this.trackIndex = (this.trackIndex + 1) % TRACKS.length;
				return this.setSession(sessionFor(this.trackIndex, 'playing', 0));
			case 'previous':
				if (pos > 3000) return this.setSession({ ...s, positionMs: 0, positionAtMs: Date.now() });
				this.trackIndex = (this.trackIndex + TRACKS.length - 1) % TRACKS.length;
				return this.setSession(sessionFor(this.trackIndex, 'playing', 0));
		}
	}

	private startSimulation() {
		setInterval(() => {
			const s = this.media.sessions[0];
			if (s && s.status === 'playing' && s.durationMs && this.livePosition(s) >= s.durationMs) {
				this.mediaControl('next');
			}
		}, 1000);
		// Discord asks for attention now and then, like a real chat app.
		setInterval(() => {
			const w = this.windows.find((x) => x.appId.includes('discord'));
			if (w && !w.isFocused) {
				this.patchWindow(w.hwnd, { isFlashing: true });
				setTimeout(() => {
					const cur = this.win(w.hwnd);
					if (cur?.isFlashing) this.patchWindow(w.hwnd, { isFlashing: false });
				}, 8000);
			}
		}, 45_000);
	}

	/** Dev helper: simulate the Windows key. */
	pressStartKey() {
		this.record('dev', 'event:native://shell startKey', {});
		this.emit('native://shell', { type: 'startKey' });
	}

	/** Dev helper: add or remove a fake window. */
	spawnRandomWindow() {
		const pool = this.apps.filter((a) => a.exePath);
		const app = pool[Math.floor(Math.random() * pool.length)];
		this.openWindow(app, `${app.name} ${Math.floor(Math.random() * 90 + 10)}`);
	}

	// ------------------------------------------------------------ mods

	private async refreshMods(emitIfChanged: boolean) {
		let list: { manifest: ModManifest; baseUrl: string }[] = [];
		try {
			const res = await fetch('/__arui/dev/mods.json', { cache: 'no-store' });
			if (res.ok) list = await res.json();
		} catch {
			// No dev server (static build): only built-in mods.
		}
		const next: InstalledMod[] = list.map(({ manifest, baseUrl }) => ({
			manifest,
			baseUrl,
			builtin: false,
			enabled: this.enabled[manifest.id] ?? DEFAULT_ENABLED.has(manifest.id)
		}));
		const changed = JSON.stringify(next) !== JSON.stringify(this.mods);
		this.mods = next;
		if (changed && emitIfChanged) this.emit('arui://mods-changed', clone(this.mods));
	}

	// ------------------------------------------------------------ surfaces

	get primary(): MonitorInfo {
		return this.monitors.find((m) => m.primary) ?? this.monitors[0];
	}

	private defineSurfaces(specs: SurfaceSpec[]) {
		this.specs = specs;
		const ids = new Set(specs.map((s) => s.id));
		for (const [label, inst] of this.instances) {
			if (!ids.has(inst.id)) this.instances.delete(label);
			else inst.spec = specs.find((s) => s.id === inst.id)!;
		}
		for (const spec of specs) {
			const w = spec.window;
			if (w.kind === 'appbar' && w.autoOpen !== false) {
				// The dev desktop only renders the primary monitor.
				const monitor = w.perMonitor === false ? null : this.primary.id;
				const label = monitor ? `${spec.id}:${monitor}` : spec.id;
				if (!this.instances.has(label)) {
					this.instances.set(label, {
						label,
						id: spec.id,
						monitor,
						spec,
						visible: true,
						params: {},
						rect: null,
						size: {},
						frame: { x: 0, y: 0, width: 0, height: 0 }
					});
				}
			} else if (w.autoOpen && !this.instances.has(spec.id)) {
				void this.openSurface({ id: spec.id });
			}
		}
		this.layout();
	}

	instanceFor(id: string, monitor?: string | null): SurfaceInstance | undefined {
		const spec = this.specs.find((s) => s.id === id);
		if (!spec) return undefined;
		const perMonitor = spec.window.kind === 'appbar' && spec.window.perMonitor !== false;
		return this.instances.get(perMonitor ? `${id}:${monitor ?? this.primary.id}` : id);
	}

	private openSurface(args: Commands['surface_open']['args']) {
		const spec = this.specs.find((s) => s.id === args.id);
		if (!spec) throw new Error(`surface '${args.id}' is not defined`);
		const perMonitor = spec.window.kind === 'appbar' && spec.window.perMonitor !== false;
		const monitor = perMonitor ? (args.monitor ?? this.primary.id) : (args.monitor ?? null);
		const label = perMonitor ? `${spec.id}:${monitor}` : spec.id;
		const prev = this.instances.get(label);
		const inst: SurfaceInstance = {
			label,
			id: spec.id,
			monitor,
			spec,
			visible: true,
			params: args.params ?? {},
			rect: args.rect ?? null,
			size: prev?.size ?? {},
			frame: prev?.frame ?? { x: 0, y: 0, width: 0, height: 0 }
		};
		this.instances.set(label, inst);
		this.layout();
		this.emit('arui://surface-shown', { params: clone(inst.params) }, label);
	}

	closeSurface(id: string, monitor?: string | null, byBlur = false) {
		const inst = this.instanceFor(id, monitor);
		if (!inst || !inst.visible) return;
		inst.visible = false;
		if (byBlur) this.blurClosedAt.set(id, Date.now());
		this.layout();
		this.emit('arui://surface-hidden', {}, inst.label);
	}

	/** Compute window frames (CSS px on the dev desktop) and the primary work area. */
	private layout() {
		const mon = this.primary;
		const b = mon.bounds;
		const wa: Rect = { ...b };
		const bars = [...this.instances.values()].filter(
			(i) => i.spec.window.kind === 'appbar' && i.visible
		);
		for (const bar of bars) {
			const w = bar.spec.window;
			const t = bar.size.thickness ?? w.thickness ?? 48;
			const edge = w.edge ?? 'bottom';
			const reserve = w.reserve !== false;
			if (edge === 'bottom') {
				bar.frame = { x: wa.left, y: wa.bottom - t, width: wa.right - wa.left, height: t };
				if (reserve) wa.bottom -= t;
			} else if (edge === 'top') {
				bar.frame = { x: wa.left, y: wa.top, width: wa.right - wa.left, height: t };
				if (reserve) wa.top += t;
			} else if (edge === 'left') {
				bar.frame = { x: wa.left, y: wa.top, width: t, height: wa.bottom - wa.top };
				if (reserve) wa.left += t;
			} else {
				bar.frame = { x: wa.right - t, y: wa.top, width: t, height: wa.bottom - wa.top };
				if (reserve) wa.right -= t;
			}
		}
		const changed = JSON.stringify(wa) !== JSON.stringify(mon.workArea);
		if (changed) {
			this.monitors = this.monitors.map((m) => (m.id === mon.id ? { ...m, workArea: wa } : m));
			this.emit('native://shell', { type: 'monitorsChanged', data: clone(this.monitors) });
		}
		for (const inst of this.instances.values()) {
			if (inst.spec.window.kind !== 'appbar') inst.frame = this.popupFrame(inst, wa);
		}
		this.notify();
	}

	private popupFrame(inst: SurfaceInstance, wa: Rect): Frame {
		const w = inst.spec.window;
		const width = inst.size.width ?? w.width ?? 400;
		const height = inst.size.height ?? w.height ?? 480;
		const off = w.offset ?? 8;
		const anchor = w.anchor ?? 'center';
		let x = (wa.left + wa.right - width) / 2;
		let y = (wa.top + wa.bottom - height) / 2;
		const r = inst.rect;
		if ((anchor === 'rect' || anchor === 'cursor') && r) {
			const cx = (r.left + r.right) / 2;
			const H = this.primary.bounds.bottom;
			const W = this.primary.bounds.right;
			if (anchor === 'cursor') {
				x = r.left;
				y = r.top - height;
				if (y < wa.top) y = r.bottom;
			} else if (r.top > H * 0.66) {
				x = cx - width / 2;
				y = r.top - height - off;
			} else if (r.bottom < H * 0.33) {
				x = cx - width / 2;
				y = r.bottom + off;
			} else if (r.left < W / 2) {
				x = r.right + off;
				y = (r.top + r.bottom) / 2 - height / 2;
			} else {
				x = r.left - width - off;
				y = (r.top + r.bottom) / 2 - height / 2;
			}
		} else if (anchor.startsWith('bottom') || anchor.startsWith('top')) {
			y = anchor.startsWith('bottom') ? wa.bottom - height - off : wa.top + off;
			if (anchor.endsWith('Left')) x = wa.left + off;
			else if (anchor.endsWith('Right')) x = wa.right - width - off;
		}
		// Keep popups inside the work area with a small margin, like the real host should.
		const m = w.kind === 'popup' ? 6 : 0;
		x = Math.max(wa.left + m, Math.min(x, wa.right - width - m));
		y = Math.max(wa.top + m, Math.min(y, wa.bottom - height - m));
		return { x: Math.round(x), y: Math.round(y), width, height };
	}
}

/** Commands that run constantly and would flood the dev log. */
const QUIET = new Set<CommandName>(['surface_params', 'surface_visible', 'preview_show', 'surface_resize']);

declare global {
	interface Window {
		__ARUI_MOCK_SERVER__?: MockServer;
	}
}

/** The dev page's server if we are inside it, otherwise a private one for this window. */
export function getMockServer(): MockServer {
	try {
		const parent = window.parent !== window ? window.parent.__ARUI_MOCK_SERVER__ : undefined;
		if (parent) return parent;
	} catch {
		// Cross-origin parent: fall through to a local server.
	}
	if (!window.__ARUI_MOCK_SERVER__) window.__ARUI_MOCK_SERVER__ = new MockServer();
	return window.__ARUI_MOCK_SERVER__;
}
