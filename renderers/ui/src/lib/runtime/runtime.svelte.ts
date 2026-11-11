// The mod runtime of one window: registries (widgets, surfaces, themes, commands, events,
// settings schemas, context menu extenders), mod loading/unloading/hot reload and the
// per-mod AruiContext objects.

import { flushSync, mount, unmount, untrack, type Component } from 'svelte';
import {
	API_VERSION,
	type AruiActions,
	type AruiContext,
	type ContextMenuTarget,
	type ContextMenuTargets,
	type Dispose,
	type InstalledMod,
	type MenuItem,
	type ModManifest,
	type ModModule,
	type SettingsSchema,
	type SurfaceDefinition,
	type SurfaceInfo,
	type SurfaceOpenArgs,
	type SurfaceSpec,
	type ThemeDefinition,
	type WidgetEntry
} from '@arui/sdk';
import type { MenuPoint, SlotItem, UiHost } from '@arui/sdk/ui';
import type { Backend, SurfaceLocation } from '../backend';
import { createActions } from './actions';
import { SettingsStore } from './settings.svelte';
import { Store } from './state.svelte';

export interface BuiltinMod {
	manifest: ModManifest;
	module: ModModule;
	/** Cannot be disabled from the settings page. */
	required?: boolean;
}

interface WidgetReg {
	entry: WidgetEntry;
	ctx: AruiContext;
	seq: number;
}
interface SurfaceReg {
	def: SurfaceDefinition;
	modId: string;
	ctx: AruiContext;
	seq: number;
}
interface ThemeReg {
	theme: ThemeDefinition;
	modId: string;
	seq: number;
}
interface MenuExt {
	target: ContextMenuTarget;
	build: (data: any) => MenuItem[];
	modId: string;
}
interface LoadedMod {
	info: InstalledMod;
	ctx: AruiContext;
	disposers: Dispose[];
	styles: HTMLElement[];
}

/** Serializable menu item sent to the 'menu' popup surface. */
export interface WireMenuItem {
	id: string;
	label: string;
	icon?: string;
	disabled?: boolean;
	separatorBefore?: boolean;
	children?: WireMenuItem[];
}

export const RUNTIME_INFO_COMMAND = 'arui.runtime';

/** What `ctx.commands.run('arui.runtime')` resolves to. */
export interface RuntimeInfo {
	readonly schemas: Record<string, SettingsSchema>;
	readonly errors: Record<string, string>;
	readonly themes: { id: string; name: string; modId: string; tokens: Record<string, string> }[];
	readonly surfaces: SurfaceSpec[];
	isRequired(id: string): boolean;
	setModEnabled(id: string, enabled: boolean): Promise<void>;
}

export const MENU_SURFACE = 'menu';
export const MENU_SELECT_EVENT = 'core.menus:select';

/** Events the runtime raises locally for native notifications that have no AruiState field. */
export const LOCAL_EVENTS = {
	startKey: 'shell.startKey',
	explorerRestarted: 'shell.explorerRestarted',
	fullscreen: 'shell.fullscreen',
	trayBalloon: 'tray.balloon',
	surfaceShown: 'surface.shown',
	surfaceHidden: 'surface.hidden'
} as const;

class SurfaceInfoImpl implements SurfaceInfo {
	params = $state<Record<string, unknown>>({});
	visible = $state(true);
	constructor(
		private backend: Backend,
		public id: string,
		public monitor: string | null
	) {}
	resize(size: { width?: number; height?: number; thickness?: number }) {
		return this.backend.invoke('surface_resize', size);
	}
	close() {
		return this.backend.invoke('surface_close', { id: this.id, monitor: this.monitor ?? undefined });
	}
}

let modSeq = 0;
const nextSeq = () => ++modSeq;

export class Runtime implements UiHost {
	readonly settings: SettingsStore;
	readonly state: Store;
	readonly actions: AruiActions;
	readonly surface: SurfaceInfoImpl;

	widgetRegs = $state.raw<WidgetReg[]>([]);
	hidden = $state.raw<{ id: string; seq: number }[]>([]);
	surfaceRegs = $state.raw<SurfaceReg[]>([]);
	themeRegs = $state.raw<ThemeReg[]>([]);
	schemas = $state.raw<Record<string, SettingsSchema>>({});
	errors = $state<Record<string, string>>({});
	/** True once the initial set of mods is loaded. */
	ready = $state(false);

	private menuExts: MenuExt[] = [];
	private commands = new Map<string, { handler: (...a: any[]) => unknown; seq: number }[]>();
	private events = new Map<string, Set<(payload: any, from: string) => void>>();
	private loaded = new Map<string, LoadedMod>();
	private external: InstalledMod[] = [];
	private queue: Promise<unknown> = Promise.resolve();
	private pendingMenus = new Map<string, MenuItem[]>();

	constructor(
		readonly backend: Backend,
		readonly location: SurfaceLocation,
		readonly builtins: BuiltinMod[]
	) {
		this.settings = new SettingsStore(backend);
		this.state = new Store(backend, this.settings);
		this.actions = createActions(backend, this.state);
		this.surface = new SurfaceInfoImpl(backend, location.surface, location.monitor);
	}

	get isHost() {
		return this.location.surface === 'host';
	}

	// ------------------------------------------------------------ boot

	async init() {
		const b = this.backend;
		await this.settings.init();
		await this.state.init();

		await b.listen('arui://broadcast', ({ name, payload, from }) => this.dispatch(name, payload, from));
		await b.listen('arui://surface-shown', ({ params }) => {
			this.surface.params = params ?? {};
			this.surface.visible = true;
			this.dispatch(LOCAL_EVENTS.surfaceShown, params, b.label);
		});
		await b.listen('arui://surface-hidden', () => {
			this.surface.visible = false;
			this.dispatch(LOCAL_EVENTS.surfaceHidden, {}, b.label);
		});
		await b.listen('arui://mods-changed', (list) => this.enqueue(() => this.sync(list)));
		await b.listen('arui://mod-changed', ({ id }) => this.enqueue(() => this.reload(id)));
		this.state.onShell((e) => {
			if (e.type === 'startKey') this.dispatch(LOCAL_EVENTS.startKey, {}, 'host');
			else if (e.type === 'explorerRestarted') this.dispatch(LOCAL_EVENTS.explorerRestarted, {}, 'host');
			else if (e.type === 'fullscreen') this.dispatch(LOCAL_EVENTS.fullscreen, e.data, 'host');
		});
		this.state.onTray((e) => {
			if (e.type === 'balloon') this.dispatch(LOCAL_EVENTS.trayBalloon, e.data, 'host');
		});
		this.settings.onChange((ns, key) => {
			if (ns === 'core' && key === 'disabledBuiltins') this.enqueue(() => this.sync(this.external));
		});
		this.on(MENU_SELECT_EVENT, (p: { token: string; path: string[] }) => this.menuSelected(p));
		// Runtime introspection for the settings page (the mod API has no field for these yet).
		this.commands.set(RUNTIME_INFO_COMMAND, [{ handler: () => this.info(), seq: 0 }]);

		if (!this.isHost && this.location.surface !== 'dev') {
			this.surface.visible = document.visibilityState !== 'hidden';
			try {
				this.surface.params = (await b.invoke('surface_params')) ?? {};
			} catch {
				this.surface.params = {};
			}
		}

		let external: InstalledMod[] = [];
		try {
			external = await b.invoke('mods_list');
		} catch (err) {
			console.warn('[arui] mods_list failed', err);
		}
		await this.enqueue(() => this.sync(external));
		this.ready = true;
		this.startThemeEffect();
		if (this.isHost) this.startSurfaceSync();
	}

	/** Live (reactive getters) view returned by the `arui.runtime` command. */
	info(): RuntimeInfo {
		const rt = this;
		return {
			get schemas() {
				return rt.schemas;
			},
			get errors() {
				return rt.errors;
			},
			get themes() {
				const seen = new Map<string, { id: string; name: string; modId: string; tokens: Record<string, string> }>();
				for (const r of [...rt.themeRegs].sort((a, b) => a.seq - b.seq)) {
					seen.set(r.theme.id, { id: r.theme.id, name: r.theme.name, modId: r.modId, tokens: r.theme.tokens ?? {} });
				}
				return [...seen.values()];
			},
			get surfaces() {
				return rt.surfaceSpecs();
			},
			isRequired: (id) => rt.isRequired(id),
			setModEnabled: (id, enabled) => rt.setModEnabled(id, enabled)
		};
	}

	private enqueue<T>(job: () => Promise<T>): Promise<T> {
		const run = this.queue.then(job, job);
		this.queue = run.catch(() => undefined);
		return run;
	}

	// ------------------------------------------------------------ mod list

	disabledBuiltins(): string[] {
		return this.settings.get<string[]>('core', 'disabledBuiltins') ?? [];
	}

	/** Every known mod (built-ins first) with its effective enabled state. */
	allMods(): InstalledMod[] {
		const disabled = this.disabledBuiltins();
		const builtins: InstalledMod[] = this.builtins.map((b) => ({
			manifest: b.manifest,
			baseUrl: `/builtin/${b.manifest.id}/`,
			builtin: true,
			enabled: b.required || !disabled.includes(b.manifest.id)
		}));
		const ids = new Set(builtins.map((m) => m.manifest.id));
		return [...builtins, ...this.external.filter((m) => !ids.has(m.manifest.id))];
	}

	isRequired(id: string) {
		return !!this.builtins.find((b) => b.manifest.id === id)?.required;
	}

	async setModEnabled(id: string, enabled: boolean) {
		const builtin = this.builtins.find((b) => b.manifest.id === id);
		if (builtin) {
			if (builtin.required) return;
			const set = new Set(this.disabledBuiltins());
			if (enabled) set.delete(id);
			else set.add(id);
			await this.settings.set('core', 'disabledBuiltins', [...set]);
		} else {
			await this.backend.invoke('mod_set_enabled', { id, enabled });
		}
	}

	private async sync(external: InstalledMod[]) {
		this.external = external;
		const all = this.allMods();
		this.state.mods = all;
		const wanted = all.filter((m) => m.enabled);
		const wantedIds = new Set(wanted.map((m) => m.manifest.id));
		for (const id of [...this.loaded.keys()].reverse()) {
			if (!wantedIds.has(id)) this.unload(id);
		}
		for (const m of wanted) {
			if (!this.loaded.has(m.manifest.id)) await this.load(m);
		}
	}

	private async reload(id: string) {
		const current = this.loaded.get(id);
		if (!current || current.info.builtin) return;
		this.unload(id);
		const info = this.external.find((m) => m.manifest.id === id) ?? current.info;
		await this.load(info, true);
		console.info(`[arui] hot reloaded ${id}`);
	}

	// ------------------------------------------------------------ load / unload

	private modUrl(info: InstalledMod, path: string): string {
		const base = new URL(info.baseUrl, location.href);
		return new URL(path.replace(/^\.?\//, ''), base).href;
	}

	private async load(info: InstalledMod, bust = false) {
		const { manifest } = info;
		const id = manifest.id;
		delete this.errors[id];
		if (manifest.apiVersion !== API_VERSION) {
			this.errors[id] = `built for API v${manifest.apiVersion}, this ARUI provides v${API_VERSION}`;
			console.warn(`[arui] skipping ${id}: ${this.errors[id]}`);
			return;
		}
		const t = Date.now();
		let mod: ModModule | undefined;
		try {
			if (info.builtin) {
				mod = this.builtins.find((b) => b.manifest.id === id)?.module;
			} else {
				const url = new URL(this.modUrl(info, manifest.entry ?? 'dist/index.js'));
				if (bust) url.searchParams.set('t', String(t));
				const ns = await import(/* @vite-ignore */ url.href);
				mod = typeof ns.activate === 'function' ? ns : ns.default;
			}
			if (!mod || typeof mod.activate !== 'function') throw new Error('entry has no activate()');
		} catch (err) {
			this.errors[id] = `failed to load: ${(err as Error).message ?? err}`;
			console.error(`[arui] ${id}`, err);
			return;
		}

		const styles: HTMLElement[] = [];
		for (const css of manifest.styles ?? []) {
			const link = document.createElement('link');
			link.rel = 'stylesheet';
			const href = new URL(this.modUrl(info, css));
			href.searchParams.set('t', String(t));
			link.href = href.href;
			link.dataset.mod = id;
			document.head.appendChild(link);
			styles.push(link);
		}

		const record: LoadedMod = { info, ctx: null as unknown as AruiContext, disposers: [], styles };
		record.ctx = this.createContext(record);
		this.loaded.set(id, record);
		try {
			const ret = await mod.activate(record.ctx);
			if (typeof ret === 'function') record.disposers.push(ret);
		} catch (err) {
			this.errors[id] = `activate() failed: ${(err as Error).message ?? err}`;
			console.error(`[arui] ${id} activate failed`, err);
			this.unload(id);
		}
	}

	private unload(id: string) {
		const rec = this.loaded.get(id);
		if (!rec) return;
		this.loaded.delete(id);
		for (const fn of rec.disposers.splice(0).reverse()) {
			try {
				fn();
			} catch (err) {
				console.error(`[arui] ${id} dispose failed`, err);
			}
		}
		rec.styles.forEach((el) => el.remove());
	}

	// ------------------------------------------------------------ registries (UiHost)

	slot(name: string): SlotItem[] {
		const latest = new Map<string, WidgetReg>();
		for (const r of this.widgetRegs) {
			if (r.entry.slot !== name) continue;
			const prev = latest.get(r.entry.id);
			if (!prev || prev.seq < r.seq) latest.set(r.entry.id, r);
		}
		const hidden = new Set(this.hidden.map((h) => h.id));
		return [...latest.values()]
			.filter((r) => !hidden.has(r.entry.id))
			.sort((a, b) => a.entry.order - b.entry.order || a.seq - b.seq)
			.map((r) => ({ entry: r.entry, ctx: r.ctx }));
	}

	/** Current provider of a surface id (the latest registration wins). */
	surfaceDef(id: string): SurfaceReg | null {
		let best: SurfaceReg | null = null;
		for (const r of this.surfaceRegs) if (r.def.id === id && (!best || r.seq > best.seq)) best = r;
		return best;
	}

	surfaceSpecs(): SurfaceSpec[] {
		const ids = [...new Set(this.surfaceRegs.map((r) => r.def.id))];
		return ids.map((id) => {
			const r = this.surfaceDef(id)!;
			return { id, modId: r.modId, window: r.def.window };
		});
	}

	menuItems<T extends ContextMenuTarget>(target: T, data: ContextMenuTargets[T]): MenuItem[] {
		const out: MenuItem[] = [];
		for (const ext of this.menuExts) {
			if (ext.target !== target) continue;
			try {
				out.push(...ext.build(data));
			} catch (err) {
				console.error(`[arui] ${ext.modId} context menu extender failed`, err);
			}
		}
		return out;
	}

	screenRect(r: { left: number; top: number; right: number; bottom: number }) {
		return this.backend.screenRect(r);
	}

	preferPopupMenus(): boolean {
		return (window.innerHeight < 360 || window.innerWidth < 360) && !!this.surfaceDef(MENU_SURFACE);
	}

	popupMenu(items: MenuItem[], at: MenuPoint) {
		const token = `${this.backend.label}#${Math.random().toString(36).slice(2, 10)}`;
		this.pendingMenus.clear();
		this.pendingMenus.set(token, items);
		const rect = this.backend.screenRect({
			left: at.clientX,
			top: at.clientY,
			right: at.clientX + 1,
			bottom: at.clientY + 1
		});
		void this.backend
			.invoke('surface_open', {
				id: MENU_SURFACE,
				monitor: this.location.monitor ?? undefined,
				rect,
				params: { token, items: toWire(items) }
			})
			.catch((err) => console.error('[arui] menu popup failed', err));
	}

	private menuSelected({ token, path }: { token: string; path: string[] }) {
		const items = this.pendingMenus.get(token);
		if (!items) return;
		this.pendingMenus.delete(token);
		let list: MenuItem[] | undefined = items;
		let item: MenuItem | undefined;
		for (const id of path) {
			item = list?.find((i) => i.id === id);
			list = item?.children;
		}
		try {
			item?.onSelect();
		} catch (err) {
			console.error('[arui] menu item failed', err);
		}
	}

	// ------------------------------------------------------------ events

	on(name: string, handler: (payload: any, from: string) => void): Dispose {
		let set = this.events.get(name);
		if (!set) this.events.set(name, (set = new Set()));
		set.add(handler);
		return () => set!.delete(handler);
	}

	private dispatch(name: string, payload: unknown, from: string) {
		for (const fn of [...(this.events.get(name) ?? [])]) {
			try {
				fn(payload, from);
			} catch (err) {
				console.error(`[arui] event handler for ${name} failed`, err);
			}
		}
	}

	// ------------------------------------------------------------ themes

	activeTheme(): string {
		return this.settings.get<string>('core', 'theme') ?? 'default';
	}

	private startThemeEffect() {
		$effect.root(() => {
			$effect(() => {
				const want = this.activeTheme();
				const pick = (id: string) =>
					this.themeRegs.filter((r) => r.theme.id === id).sort((a, b) => b.seq - a.seq)[0];
				const reg = pick(want) ?? pick('default');
				applyTheme(reg?.theme ?? null);
			});
		});
	}

	private startSurfaceSync() {
		let last = '';
		let timer: ReturnType<typeof setTimeout> | undefined;
		$effect.root(() => {
			$effect(() => {
				const specs = this.surfaceSpecs();
				const json = JSON.stringify(specs);
				clearTimeout(timer);
				timer = setTimeout(() => {
					if (json === last) return;
					last = json;
					this.backend
						.invoke('surfaces_define', { specs: JSON.parse(json) })
						.catch((err) => console.error('[arui] surfaces_define failed', err));
				}, 30);
			});
		});
	}

	// ------------------------------------------------------------ context

	private createContext(rec: LoadedMod): AruiContext {
		const rt = this;
		const manifest = rec.info.manifest;
		const modId = manifest.id;
		let widgetCount = 0;
		const track = (fn: Dispose): Dispose => {
			let done = false;
			const wrapped = () => {
				if (done) return;
				done = true;
				const i = rec.disposers.indexOf(wrapped);
				if (i >= 0) rec.disposers.splice(i, 1);
				fn();
			};
			rec.disposers.push(wrapped);
			return wrapped;
		};
		const openArgs = (a: SurfaceOpenArgs | string): SurfaceOpenArgs =>
			typeof a === 'string' ? { id: a } : a;
		const prefix = `[${modId}]`;

		const ctx: AruiContext = {
			mod: manifest,
			apiVersion: API_VERSION,
			state: rt.state,
			actions: rt.actions,
			surface: rt.surface,

			widgets: {
				register(slot, component, opts) {
					const reg: WidgetReg = {
						entry: {
							id: opts?.id ?? `${modId}:${widgetCount++}`,
							modId,
							slot,
							order: opts?.order ?? 500,
							component: component as Component<any>,
							props: (opts?.props ?? {}) as Record<string, unknown>
						},
						ctx,
						seq: nextSeq()
					};
					untrack(() => (rt.widgetRegs = [...rt.widgetRegs, reg]));
					return track(() => untrack(() => (rt.widgetRegs = rt.widgetRegs.filter((r) => r !== reg))));
				},
				hide(widgetId) {
					const h = { id: widgetId, seq: nextSeq() };
					untrack(() => (rt.hidden = [...rt.hidden, h]));
					return track(() => untrack(() => (rt.hidden = rt.hidden.filter((x) => x !== h))));
				}
			},

			surfaces: {
				register(def) {
					const reg: SurfaceReg = { def, modId, ctx, seq: nextSeq() };
					untrack(() => (rt.surfaceRegs = [...rt.surfaceRegs, reg]));
					return track(() => untrack(() => (rt.surfaceRegs = rt.surfaceRegs.filter((r) => r !== reg))));
				},
				open: (a) => rt.backend.invoke('surface_open', openArgs(a)),
				close: (id, monitor) => rt.backend.invoke('surface_close', { id, monitor }),
				toggle: (a) => rt.backend.invoke('surface_toggle', openArgs(a))
			},

			themes: {
				register(theme) {
					const reg: ThemeReg = { theme, modId, seq: nextSeq() };
					untrack(() => (rt.themeRegs = [...rt.themeRegs, reg]));
					return track(() => untrack(() => (rt.themeRegs = rt.themeRegs.filter((r) => r !== reg))));
				},
				activate(id) {
					void rt.settings.set('core', 'theme', id);
				},
				get active() {
					return rt.activeTheme();
				}
			},

			commands: {
				register(id, handler) {
					const entry = { handler, seq: nextSeq() };
					const list = rt.commands.get(id) ?? [];
					list.push(entry);
					rt.commands.set(id, list);
					return track(() => {
						const l = rt.commands.get(id) ?? [];
						rt.commands.set(
							id,
							l.filter((e) => e !== entry)
						);
					});
				},
				async run(id, ...args) {
					const list = rt.commands.get(id);
					const top = list?.[list.length - 1];
					if (!top) throw new Error(`command '${id}' is not registered in this window`);
					return top.handler(...args);
				}
			},

			events: {
				on(name, handler) {
					return track(rt.on(name, handler));
				},
				emit(name, payload) {
					const plain = payload === undefined ? null : JSON.parse(JSON.stringify(payload));
					return rt.backend.broadcast({ name, payload: plain, from: `${modId}@${rt.backend.label}` });
				}
			},

			settings: {
				define(schema) {
					untrack(() => (rt.schemas = { ...rt.schemas, [modId]: schema }));
					track(() => {
						untrack(() => {
							const next = { ...rt.schemas };
							delete next[modId];
							rt.schemas = next;
						});
					});
				},
				get<T>(key: string): T {
					const v = rt.settings.get<T>(modId, key);
					if (v !== undefined) return v;
					return rt.schemas[modId]?.fields.find((f) => f.key === key)?.default as T;
				},
				set: (key, value) => rt.settings.set(modId, key, value),
				read: <T>(ns: string, key: string) => rt.settings.get<T>(ns, key) as T
			},

			contextMenus: {
				extend(target, build) {
					const ext: MenuExt = { target, build, modId };
					rt.menuExts.push(ext);
					return track(() => (rt.menuExts = rt.menuExts.filter((e) => e !== ext)));
				}
			},

			asset: (path) => rt.modUrl(rec.info, path),
			iconUrl: (key) => rt.backend.iconUrl(key),
			invoke: ((cmd: any, ...args: any[]) => (rt.backend.invoke as any)(cmd, ...args)) as AruiContext['invoke'],
			log: {
				info: (...a) => console.info(prefix, ...a),
				warn: (...a) => console.warn(prefix, ...a),
				error: (...a) => console.error(prefix, ...a)
			},
			onDispose(fn) {
				track(fn);
			}
		};
		return ctx;
	}
}

// ------------------------------------------------------------ helpers

function applyTheme(theme: ThemeDefinition | null) {
	const head = document.head;
	let style = document.getElementById('arui-theme') as HTMLStyleElement | null;
	let link = document.getElementById('arui-theme-link') as HTMLLinkElement | null;
	if (!style) {
		style = document.createElement('style');
		style.id = 'arui-theme';
		head.appendChild(style);
	}
	const tokens = Object.entries(theme?.tokens ?? {})
		.map(([k, v]) => `  --arui-${k}: ${v};`)
		.join('\n');
	style.textContent = `${tokens ? `:root {\n${tokens}\n}\n` : ''}${theme?.css ?? ''}`;
	document.documentElement.dataset.theme = theme?.id ?? 'none';
	if (theme?.url) {
		if (!link) {
			link = document.createElement('link');
			link.id = 'arui-theme-link';
			link.rel = 'stylesheet';
			head.appendChild(link);
		}
		if (link.href !== theme.url) link.href = theme.url;
	} else {
		link?.remove();
	}
}

/** Render a menu icon to a string (URL or inline SVG markup) so it can cross windows. */
function iconToString(icon: MenuItem['icon']): string | undefined {
	if (!icon) return undefined;
	if (typeof icon === 'string') return icon;
	const el = document.createElement('div');
	try {
		const inst = mount(icon, { target: el, props: { size: 16 } });
		flushSync();
		const html = el.innerHTML;
		void unmount(inst);
		return html.trim() ? `svg:${html}` : undefined;
	} catch {
		return undefined;
	}
}

function toWire(items: MenuItem[]): WireMenuItem[] {
	return items.map((i) => ({
		id: i.id,
		label: i.label,
		icon: iconToString(i.icon),
		disabled: i.disabled,
		separatorBefore: i.separatorBefore,
		children: i.children ? toWire(i.children) : undefined
	}));
}
