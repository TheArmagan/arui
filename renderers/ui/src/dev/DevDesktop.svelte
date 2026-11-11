<script lang="ts">
	// Browser-only fake desktop (`?surface=dev`). Every surface runs in its own iframe at the
	// position the mock host computed, exactly like separate windows in the real shell.
	import type { MockServer, SurfaceInstance } from '../lib/backend/mock/server';
	import { mockIconUrl } from '../lib/backend/mock/icons';
	import ChevronDown from '@lucide/svelte/icons/chevron-down';
	import ChevronUp from '@lucide/svelte/icons/chevron-up';
	import Keyboard from '@lucide/svelte/icons/keyboard';
	import Plus from '@lucide/svelte/icons/plus';
	import RotateCcw from '@lucide/svelte/icons/rotate-ccw';

	let { server }: { server: MockServer } = $props();

	let version = $state(0);
	$effect(() => server.subscribe(() => version++));

	const snap = $derived.by(() => {
		void version;
		return {
			instances: [...server.instances.values()].map((i) => ({ ...i, frame: { ...i.frame } })),
			specs: server.specs.map((s) => s.id),
			workArea: { ...server.primary.workArea },
			focused: server.windows.find((w) => w.isFocused && !w.isMinimized) ?? null,
			log: server.log.slice(0, 14)
		};
	});

	/** Popups are mounted on first open and then kept, like pre-created hidden windows. */
	let mounted = $state<Record<string, boolean>>({});
	$effect(() => {
		for (const i of snap.instances) if (i.visible && !mounted[i.label]) mounted[i.label] = true;
	});

	const wa = $derived(snap.workArea);
	let panelOpen = $state(true);
	const frames = new Map<string, HTMLIFrameElement>();

	function src(i: SurfaceInstance) {
		const q = new URLSearchParams({ surface: i.id });
		// Only per-monitor appbars carry the monitor in their URL (and window label).
		if (i.monitor && i.label !== i.id) q.set('monitor', i.monitor);
		return `/?${q}`;
	}

	function frame(node: HTMLIFrameElement, inst: SurfaceInstance) {
		frames.set(inst.label, node);
		const onLoad = () => {
			const win = node.contentWindow;
			if (!win) return;
			win.addEventListener('blur', () => {
				const cur = server.instances.get(inst.label);
				if (!cur?.visible) return;
				const w = cur.spec.window;
				if (w.kind === 'popup' && w.closeOnBlur !== false) server.closeSurface(cur.id, cur.monitor, true);
			});
			focusIfNeeded(inst.label);
		};
		node.addEventListener('load', onLoad);
		return {
			destroy() {
				frames.delete(inst.label);
				node.removeEventListener('load', onLoad);
			}
		};
	}

	function focusIfNeeded(label: string) {
		const cur = server.instances.get(label);
		if (!cur?.visible || cur.spec.window.kind === 'appbar' || cur.spec.window.focusable === false) return;
		frames.get(label)?.contentWindow?.focus();
	}

	// Focus a popup when it becomes visible so blur-to-close works.
	let lastVisible = new Set<string>();
	$effect(() => {
		const now = new Set(snap.instances.filter((i) => i.visible).map((i) => i.label));
		for (const label of now) if (!lastVisible.has(label)) setTimeout(() => focusIfNeeded(label), 30);
		lastVisible = now;
	});

	function open(id: string, extra: Record<string, unknown> = {}) {
		void server.invoke('surface_toggle', { id, ...extra }, 'dev').catch((e) => console.warn(e));
	}

	function openPreview() {
		const w = server.windows[0];
		if (!w) return;
		const wa = server.primary.workArea;
		const cx = (wa.left + wa.right) / 2;
		open('preview', {
			rect: { left: cx - 22, top: wa.bottom + 14, right: cx + 22, bottom: wa.bottom + 58 },
			params: { appId: w.appId }
		});
	}

	function resetStorage() {
		try {
			localStorage.removeItem('arui.mock.settings.v1');
			localStorage.removeItem('arui.mock.mods.v1');
		} catch {
			// ignore
		}
		location.reload();
	}

	const buttons: [string, () => void][] = [
		['Start menu', () => open('start-menu')],
		['Tray flyout', () => open('tray-flyout', { rect: trayRect() })],
		['Quick settings', () => open('quick-settings', { rect: trayRect() })],
		['Calendar', () => open('calendar', { rect: trayRect() })],
		['Preview', openPreview],
		['Settings', () => open('settings')]
	];

	function trayRect() {
		const wa = server.primary.workArea;
		const b = server.primary.bounds;
		return { left: b.right - 120, top: wa.bottom + 14, right: b.right - 80, bottom: wa.bottom + 58 };
	}

	const time = (t: number) => new Date(t).toLocaleTimeString('en-GB');
</script>

<div class="wallpaper fixed inset-0 overflow-hidden">
	<!-- Work area left over by the appbars, and a faux window of the focused app inside it. -->
	<div
		class="pointer-events-none absolute border border-dashed border-white/15"
		style:left="{wa.left + 4}px"
		style:top="{wa.top + 4}px"
		style:width="{wa.right - wa.left - 8}px"
		style:height="{wa.bottom - wa.top - 8}px"
	>
		<span class="absolute bottom-1.5 left-2 font-mono text-[10px] text-white/30">work area {wa.right - wa.left} x {wa.bottom - wa.top}</span>
	</div>
	{#if snap.focused}
		<div
			class="absolute flex flex-col overflow-hidden rounded-lg border border-white/10 bg-[#1b1c20]/95 shadow-2xl"
			style:left="{wa.left + (wa.right - wa.left) * 0.06}px"
			style:top="{wa.top + (wa.bottom - wa.top) * 0.08}px"
			style:width="{(wa.right - wa.left) * 0.56}px"
			style:height="{(wa.bottom - wa.top) * 0.66}px"
		>
			<div class="flex h-8 shrink-0 items-center gap-2 border-b border-white/5 bg-[#202126] px-3 text-xs text-white/70">
				<img src={mockIconUrl(snap.focused.iconKey)} alt="" class="size-4" />
				<span class="truncate">{snap.focused.title}</span>
			</div>
			<div class="grid flex-1 grid-cols-[220px_1fr] gap-4 p-4 opacity-60">
				<div class="rounded-md bg-white/[0.03]"></div>
				<div class="flex flex-col gap-3">
					<div class="h-10 rounded-md bg-white/[0.04]"></div>
					<div class="flex-1 rounded-md bg-white/[0.025]"></div>
				</div>
			</div>
		</div>
	{/if}

	<iframe title="host" src="/?surface=host" class="pointer-events-none absolute size-px opacity-0"></iframe>

	{#each snap.instances as inst (inst.label)}
		{#if inst.spec.window.kind === 'appbar' || mounted[inst.label]}
			<iframe
				use:frame={inst}
				title={inst.label}
				src={src(inst)}
				class="absolute border-0 bg-transparent"
				style:left="{inst.frame.x}px"
				style:top="{inst.frame.y}px"
				style:width="{inst.frame.width}px"
				style:height="{inst.frame.height}px"
				style:display={inst.visible ? 'block' : 'none'}
				style:z-index={inst.spec.window.kind === 'appbar' ? 10 : inst.id === 'menu' ? 40 : 30}
			></iframe>
		{/if}
	{/each}

	<aside
		class="absolute top-4 right-4 z-50 w-72 rounded-xl border border-white/10 bg-black/70 text-xs text-white/85 shadow-2xl backdrop-blur-md"
	>
		<button
			type="button"
			class="flex w-full items-center justify-between px-3.5 py-2.5 font-semibold"
			onclick={() => (panelOpen = !panelOpen)}
		>
			<span>ARUI dev desktop <span class="font-normal text-white/50">mock backend</span></span>
			{#if panelOpen}<ChevronUp size={14} />{:else}<ChevronDown size={14} />{/if}
		</button>
		{#if panelOpen}
			<div class="flex flex-col gap-3 border-t border-white/10 p-3.5">
				<div class="grid grid-cols-2 gap-1.5">
					{#each buttons as [label, run] (label)}
						<button
							type="button"
							class="rounded-md border border-white/10 bg-white/5 px-2 py-1.5 text-left hover:bg-white/10"
							onclick={run}>{label}</button
						>
					{/each}
				</div>
				<div class="flex gap-1.5">
					<button
						type="button"
						class="flex flex-1 items-center justify-center gap-1.5 rounded-md border border-white/10 bg-white/5 py-1.5 hover:bg-white/10"
						onclick={() => server.pressStartKey()}><Keyboard size={13} /> Win key</button
					>
					<button
						type="button"
						class="flex flex-1 items-center justify-center gap-1.5 rounded-md border border-white/10 bg-white/5 py-1.5 hover:bg-white/10"
						onclick={() => server.spawnRandomWindow()}><Plus size={13} /> Window</button
					>
					<button
						type="button"
						title="Forget mock settings and mod state"
						class="flex items-center justify-center rounded-md border border-white/10 bg-white/5 px-2 hover:bg-white/10"
						onclick={resetStorage}><RotateCcw size={13} /></button
					>
				</div>
				<div>
					<div class="mb-1 text-[10px] font-semibold tracking-wide text-white/45 uppercase">Surfaces</div>
					<div class="flex flex-wrap gap-1">
						{#each snap.specs as id (id)}
							<span class="rounded bg-white/8 px-1.5 py-0.5 font-mono text-[10px]">{id}</span>
						{/each}
					</div>
				</div>
				<div>
					<div class="mb-1 text-[10px] font-semibold tracking-wide text-white/45 uppercase">Host log</div>
					<ol class="flex max-h-56 flex-col gap-0.5 overflow-hidden font-mono text-[10px]">
						{#each snap.log as entry, i (entry.at + ':' + i)}
							<li class="truncate">
								<span class="text-white/35">{time(entry.at)}</span>
								<span class="text-white/55">{entry.from.split(':')[0]}</span>
								<span class="text-amber-200/90">{entry.cmd}</span>
								<span class="text-white/45">{JSON.stringify(entry.args)}</span>
							</li>
						{/each}
					</ol>
				</div>
			</div>
		{/if}
	</aside>
</div>

<style>
	.wallpaper {
		background:
			radial-gradient(120% 70% at 18% 108%, oklch(0.62 0.13 55 / 0.85), transparent 60%),
			radial-gradient(90% 60% at 85% 100%, oklch(0.45 0.1 20 / 0.75), transparent 65%),
			radial-gradient(100% 80% at 70% -10%, oklch(0.36 0.06 200 / 0.9), transparent 70%),
			linear-gradient(180deg, oklch(0.2 0.03 230), oklch(0.16 0.02 260));
	}
	.wallpaper::after {
		content: '';
		position: absolute;
		inset: 0;
		pointer-events: none;
		background-image: radial-gradient(oklch(1 0 0 / 0.035) 1px, transparent 1px);
		background-size: 22px 22px;
	}
</style>
