// AruiState implementation: Svelte 5 runes fed by the native://* events.

import type {
	AppEntry,
	AppGroup,
	AruiState,
	Hwnd,
	InstalledMod,
	MediaEvent,
	MediaSession,
	MonitorInfo,
	NetworkState,
	PinnedItem,
	ShellEvent,
	SystemEvent,
	TrayEvent,
	TrayIcon,
	VolumeState,
	WindowEvent,
	WindowInfo
} from '@arui/sdk';
import type { Backend } from '../backend';
import type { SettingsStore } from './settings.svelte';

const ci = (s: string | null | undefined) => (s ?? '').toLowerCase();

function exeBaseName(path: string | null | undefined): string {
	const base = (path ?? '').split(/[\\/]/).pop() ?? '';
	const name = base.replace(/\.exe$/i, '');
	return name ? name.charAt(0).toUpperCase() + name.slice(1) : '';
}

export class Store implements AruiState {
	monitors = $state<MonitorInfo[]>([]);
	windows = $state<WindowInfo[]>([]);
	focusedHwnd = $state<Hwnd | null>(null);
	apps = $state<AppEntry[]>([]);
	tray = $state<TrayIcon[]>([]);
	media = $state<{ sessions: MediaSession[]; current: string | null }>({
		sessions: [],
		current: null
	});
	volume = $state<VolumeState>({ level: 0, muted: false });
	network = $state<NetworkState>({ connected: false, internet: false, kind: 'none', name: null });
	now = $state(new Date());
	mods = $state<InstalledMod[]>([]);

	/** Order in which windows were first seen (taskbar order for running apps). */
	private firstSeen = new Map<Hwnd, number>();
	private seq = 0;
	private shellListeners = new Set<(e: ShellEvent) => void>();
	private trayListeners = new Set<(e: TrayEvent) => void>();

	constructor(
		private backend: Backend,
		private settings: SettingsStore
	) {}

	get pins(): PinnedItem[] {
		return this.settings.get<PinnedItem[]>('core', 'pins') ?? [];
	}
	set pins(value: PinnedItem[]) {
		void this.settings.set('core', 'pins', value);
	}

	focused = $derived.by(() => this.windows.find((w) => w.hwnd === this.focusedHwnd) ?? null);

	currentMedia = $derived.by(() => {
		const { sessions, current } = this.media;
		return sessions.find((s) => s.id === current) ?? sessions[0] ?? null;
	});

	groups = $derived.by(() => this.computeGroups());

	private computeGroups(): AppGroup[] {
		const apps = this.apps;
		const groups: AppGroup[] = [];
		const findApp = (appId: string | null, exe: string | null) =>
			apps.find((a) => (a.appId && ci(a.appId) === ci(appId)) || (exe && a.exePath && ci(a.exePath) === ci(exe))) ??
			null;

		for (const pin of this.pins) {
			const app =
				pin.launch.kind === 'app'
					? (apps.find((a) => a.id === (pin.launch as { id: string }).id) ?? null)
					: findApp(pin.appId, pin.launch.path);
			groups.push({
				appId: pin.appId,
				name: pin.name,
				iconKey: pin.iconKey || app?.iconKey || '',
				exePath: pin.launch.kind === 'exe' ? pin.launch.path : (app?.exePath ?? null),
				pinned: true,
				app,
				windows: [],
				isFocused: false,
				isFlashing: false
			});
		}

		const ordered = [...this.windows].sort(
			(a, b) => (this.firstSeen.get(a.hwnd) ?? 0) - (this.firstSeen.get(b.hwnd) ?? 0)
		);
		for (const w of ordered) {
			let group = groups.find(
				(g) => ci(g.appId) === ci(w.appId) || (!!g.exePath && ci(g.exePath) === ci(w.exePath))
			);
			if (!group) {
				const app = findApp(w.appId, w.exePath);
				group = {
					appId: w.appId,
					name: app?.name || exeBaseName(w.exePath) || w.title,
					iconKey: w.iconKey || app?.iconKey || '',
					exePath: w.exePath || app?.exePath || null,
					pinned: false,
					app,
					windows: [],
					isFocused: false,
					isFlashing: false
				};
				groups.push(group);
			}
			group.windows.push(w);
			if (w.hwnd === this.focusedHwnd || w.isFocused) group.isFocused = true;
			if (w.isFlashing) group.isFlashing = true;
			if (!group.iconKey) group.iconKey = w.iconKey;
		}
		return groups;
	}

	private touch(hwnd: Hwnd) {
		if (!this.firstSeen.has(hwnd)) this.firstSeen.set(hwnd, ++this.seq);
	}

	applyWindow(e: WindowEvent) {
		switch (e.type) {
			case 'snapshot':
				e.data.forEach((w) => this.touch(w.hwnd));
				this.windows = e.data;
				this.focusedHwnd = e.data.find((w) => w.isFocused)?.hwnd ?? null;
				break;
			case 'added':
			case 'updated': {
				this.touch(e.data.hwnd);
				const i = this.windows.findIndex((w) => w.hwnd === e.data.hwnd);
				if (i >= 0) this.windows[i] = e.data;
				else this.windows.push(e.data);
				if (e.data.isFocused) this.focusedHwnd = e.data.hwnd;
				break;
			}
			case 'removed':
				this.windows = this.windows.filter((w) => w.hwnd !== e.data.hwnd);
				this.firstSeen.delete(e.data.hwnd);
				if (this.focusedHwnd === e.data.hwnd) this.focusedHwnd = null;
				break;
			case 'focused':
				this.focusedHwnd = e.data.hwnd;
				for (const w of this.windows) {
					const f = w.hwnd === e.data.hwnd;
					if (w.isFocused !== f) w.isFocused = f;
				}
				break;
		}
	}

	applyTray(e: TrayEvent) {
		switch (e.type) {
			case 'snapshot':
				this.tray = e.data;
				break;
			case 'added':
			case 'updated': {
				const i = this.tray.findIndex((t) => t.id === e.data.id);
				if (i >= 0) this.tray[i] = e.data;
				else this.tray.push(e.data);
				break;
			}
			case 'removed':
				this.tray = this.tray.filter((t) => t.id !== e.data.id);
				break;
		}
		for (const fn of this.trayListeners) fn(e);
	}

	applyMedia(e: MediaEvent) {
		switch (e.type) {
			case 'snapshot':
				this.media = e.data;
				break;
			case 'updated': {
				const i = this.media.sessions.findIndex((s) => s.id === e.data.id);
				if (i >= 0) this.media.sessions[i] = e.data;
				else this.media.sessions.push(e.data);
				if (!this.media.current) this.media.current = e.data.id;
				break;
			}
			case 'removed':
				this.media.sessions = this.media.sessions.filter((s) => s.id !== e.data.id);
				if (this.media.current === e.data.id) this.media.current = this.media.sessions[0]?.id ?? null;
				break;
			case 'current':
				this.media.current = e.data.id;
				break;
		}
	}

	applySystem(e: SystemEvent) {
		if (e.type === 'volume') this.volume = e.data;
		else this.network = e.data;
	}

	applyShell(e: ShellEvent) {
		if (e.type === 'monitorsChanged') this.monitors = e.data;
		else if (e.type === 'appsChanged') this.apps = e.data;
		for (const fn of this.shellListeners) fn(e);
	}

	onShell(fn: (e: ShellEvent) => void) {
		this.shellListeners.add(fn);
		return () => this.shellListeners.delete(fn);
	}

	onTray(fn: (e: TrayEvent) => void) {
		this.trayListeners.add(fn);
		return () => this.trayListeners.delete(fn);
	}

	/** Subscribe to events first, then fetch snapshots, so nothing is missed in between. */
	async init() {
		const b = this.backend;
		await Promise.all([
			b.listen('native://window', (e) => this.applyWindow(e)),
			b.listen('native://tray', (e) => this.applyTray(e)),
			b.listen('native://media', (e) => this.applyMedia(e)),
			b.listen('native://system', (e) => this.applySystem(e)),
			b.listen('native://shell', (e) => this.applyShell(e))
		]);
		const safe = async <T>(p: Promise<T>, apply: (v: T) => void) => {
			try {
				apply(await p);
			} catch (err) {
				console.warn('[arui] initial fetch failed', err);
			}
		};
		await Promise.all([
			safe(b.invoke('windows_list'), (v) => this.applyWindow({ type: 'snapshot', data: v })),
			safe(b.invoke('apps_list'), (v) => (this.apps = v)),
			safe(b.invoke('tray_list'), (v) => (this.tray = v)),
			safe(b.invoke('media_list'), (v) => (this.media = v)),
			safe(b.invoke('volume_get'), (v) => (this.volume = v)),
			safe(b.invoke('network_get'), (v) => (this.network = v)),
			safe(b.invoke('monitors_list'), (v) => (this.monitors = v))
		]);
		// Tick at the start of every second so clocks flip on time.
		const tick = () => {
			this.now = new Date();
			setTimeout(tick, 1000 - (Date.now() % 1000) + 5);
		};
		tick();
	}
}
