<script lang="ts">
	// 'start-menu' popup: search with fuzzy filtering, pinned grid, all apps grouped by folder and
	// a power/settings footer. Fully keyboard driven (type, arrows, Enter, Escape).
	import type { AppEntry, AruiContext, MenuItem } from '@arui/sdk';
	import { ContextMenuHost, Slot } from '@arui/sdk/ui';
	import CornerDownLeft from '@lucide/svelte/icons/corner-down-left';
	import FolderOpen from '@lucide/svelte/icons/folder-open';
	import LogOut from '@lucide/svelte/icons/log-out';
	import Moon from '@lucide/svelte/icons/moon';
	import Pin from '@lucide/svelte/icons/pin';
	import PinOff from '@lucide/svelte/icons/pin-off';
	import Power from '@lucide/svelte/icons/power';
	import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
	import Search from '@lucide/svelte/icons/search';
	import Settings from '@lucide/svelte/icons/settings';
	import SquareArrowOutUpRight from '@lucide/svelte/icons/square-arrow-out-up-right';
	import { tick } from 'svelte';
	import { fuzzyFilter } from './fuzzy';

	let { ctx }: { ctx: AruiContext } = $props();

	const DEFAULT_PINS = [
		'app:explorer',
		'app:firefox',
		'app:edge',
		'app:code',
		'app:terminal',
		'app:settings',
		'app:spotify',
		'app:discord',
		'app:calculator',
		'app:photos',
		'app:notepad',
		'app:store'
	];

	let query = $state('');
	let active = $state(0);
	let input = $state<HTMLInputElement>();
	let scroller = $state<HTMLElement>();
	let powerOpen = $state(false);

	const apps = $derived([...ctx.state.apps].sort((a, b) => a.name.localeCompare(b.name)));
	const pinnedIds = $derived(ctx.settings.get<string[] | null>('pinned') ?? null);
	const pinned = $derived.by(() => {
		const ids = pinnedIds ?? DEFAULT_PINS;
		const list = ids.map((id) => apps.find((a) => a.id === id)).filter((a): a is AppEntry => !!a);
		return list.length || pinnedIds ? list : apps.slice(0, 12);
	});
	const folders = $derived.by(() => {
		const map = new Map<string, AppEntry[]>();
		for (const a of apps) {
			const key = a.folder ?? '';
			if (!map.has(key)) map.set(key, []);
			map.get(key)!.push(a);
		}
		return [...map.entries()].sort(([a], [b]) => (a === '' ? -1 : b === '' ? 1 : a.localeCompare(b)));
	});
	const results = $derived(query.trim() ? fuzzyFilter(apps, query, (a) => `${a.name} ${a.folder ?? ''}`).slice(0, 40) : []);

	/** Launchable entries in visual order, for keyboard navigation. */
	const nav = $derived(
		query.trim()
			? results.map((app) => ({ key: 'r:' + app.id, app }))
			: [
					...pinned.map((app) => ({ key: 'p:' + app.id, app })),
					...folders.flatMap(([, list]) => list.map((app) => ({ key: 'l:' + app.id, app })))
				]
	);
	const activeKey = $derived(nav[active]?.key);
	const indexOf = (key: string) => nav.findIndex((n) => n.key === key);

	// Fresh state each time the menu opens.
	$effect(() => {
		if (ctx.surface.visible) {
			query = '';
			active = 0;
			powerOpen = false;
			scroller?.scrollTo({ top: 0 });
			tick().then(() => input?.focus());
		}
	});
	$effect(() => {
		void query;
		active = 0;
	});

	function launch(app: AppEntry) {
		void ctx.actions.launchApp(app.id);
		void ctx.surface.close();
	}

	function keydown(e: KeyboardEvent) {
		if (e.key === 'ArrowDown' || (e.key === 'Tab' && !e.shiftKey)) {
			active = Math.min(nav.length - 1, active + 1);
		} else if (e.key === 'ArrowUp' || (e.key === 'Tab' && e.shiftKey)) {
			active = Math.max(0, active - 1);
		} else if (e.key === 'Enter') {
			const entry = nav[active];
			if (entry) launch(entry.app);
		} else if (e.key === 'Escape') {
			if (query) query = '';
			else void ctx.surface.close();
		} else return;
		e.preventDefault();
		tick().then(() =>
			document.querySelector(`[data-nav-key="${CSS.escape(activeKey ?? '')}"]`)?.scrollIntoView({ block: 'nearest' })
		);
	}

	function setStartPins(ids: string[]) {
		void ctx.settings.set('pinned', ids);
	}

	function appMenu(app: AppEntry): MenuItem[] {
		const onStart = pinned.some((p) => p.id === app.id);
		const onTaskbar = ctx.state.pins.some((p) => p.id === app.id);
		return [
			{ id: 'open', label: 'Open', icon: SquareArrowOutUpRight, onSelect: () => launch(app) },
			onStart
				? {
						id: 'unpin-start',
						label: 'Unpin from Start',
						icon: PinOff,
						separatorBefore: true,
						onSelect: () => setStartPins(pinned.filter((p) => p.id !== app.id).map((p) => p.id))
					}
				: {
						id: 'pin-start',
						label: 'Pin to Start',
						icon: Pin,
						separatorBefore: true,
						onSelect: () => setStartPins([...pinned.map((p) => p.id), app.id])
					},
			onTaskbar
				? { id: 'unpin-taskbar', label: 'Unpin from taskbar', icon: PinOff, onSelect: () => ctx.actions.unpin(app.id) }
				: {
						id: 'pin-taskbar',
						label: 'Pin to taskbar',
						icon: Pin,
						onSelect: () =>
							ctx.actions.pin({
								id: app.id,
								appId: app.appId ?? app.exePath?.toLowerCase() ?? app.id,
								name: app.name,
								iconKey: app.iconKey,
								launch: { kind: 'app', id: app.id }
							})
					}
		];
	}

	const powerItems = [
		{ id: 'sleep', label: 'Sleep', icon: Moon, run: () => ctx.actions.shellExecute('rundll32.exe', 'powrprof.dll,SetSuspendState 0,1,0') },
		{ id: 'restart', label: 'Restart', icon: RotateCcw, run: () => ctx.actions.shellExecute('shutdown.exe', '/r /t 0') },
		{ id: 'shutdown', label: 'Shut down', icon: Power, run: () => ctx.actions.shellExecute('shutdown.exe', '/s /t 0') },
		{ id: 'restart-arui', label: 'Restart ARUI', icon: RotateCcw, run: () => ctx.actions.restart() },
		{ id: 'quit-arui', label: 'Quit ARUI', icon: LogOut, run: () => ctx.actions.quit() }
	];
</script>

{#snippet appRow(app: AppEntry, key: string)}
	<ContextMenuHost target="start.app" data={{ app }} items={() => appMenu(app)}>
		<button
			type="button"
			data-nav-key={key}
			class="flex w-full items-center gap-3 rounded-md px-2.5 py-1.5 text-left text-sm transition-colors
				{activeKey === key ? 'bg-accent text-accent-foreground' : 'hover:bg-muted/60'}"
			onmouseenter={() => (active = indexOf(key))}
			onclick={() => launch(app)}
		>
			<img src={ctx.iconUrl(app.iconKey)} alt="" class="size-5" draggable="false" />
			<span class="min-w-0 flex-1 truncate">{app.name}</span>
			{#if activeKey === key && query}
				<CornerDownLeft class="text-muted-foreground size-3.5" />
			{/if}
		</button>
	</ContextMenuHost>
{/snippet}

<div class="h-full p-px">
	<div class="arui-surface flex h-full flex-col overflow-hidden">
		<div class="px-4 pt-4 pb-2.5">
			<label
				class="bg-background/70 focus-within:border-highlight/70 flex items-center gap-3 rounded-full border px-3.5 py-2 transition-colors"
			>
				<Search class="text-muted-foreground size-4 shrink-0" />
				<input
					bind:this={input}
					bind:value={query}
					onkeydown={keydown}
					type="text"
					spellcheck="false"
					placeholder="Search apps"
					class="placeholder:text-muted-foreground min-w-0 flex-1 bg-transparent text-sm outline-none"
				/>
			</label>
			<Slot name="start.header" />
		</div>

		<div bind:this={scroller} class="scrollbar-thin min-h-0 flex-1 overflow-y-auto px-4 pb-3">
			{#if query.trim()}
				<div class="text-muted-foreground mb-1.5 px-1 text-xs font-medium tracking-wide uppercase">
					Best matches
				</div>
				{#each results as app (app.id)}
					{@render appRow(app, 'r:' + app.id)}
				{:else}
					<div class="text-muted-foreground px-2 py-10 text-center text-sm">
						No apps match "{query}"
					</div>
				{/each}
			{:else}
				<section class="mb-5">
					<div class="mb-2 flex items-baseline justify-between px-1">
						<h2 class="text-sm font-semibold">Pinned</h2>
						<span class="text-muted-foreground text-[11px]">Right click an app to pin or unpin</span>
					</div>
					<div class="grid grid-cols-6 gap-1">
						{#each pinned as app (app.id)}
							<ContextMenuHost target="start.app" data={{ app }} items={() => appMenu(app)}>
								<button
									type="button"
									data-nav-key={'p:' + app.id}
									class="flex w-full flex-col items-center gap-1.5 rounded-lg px-1 py-2.5 transition-colors
										{activeKey === 'p:' + app.id ? 'bg-accent' : 'hover:bg-muted/60'}"
									onmouseenter={() => (active = indexOf('p:' + app.id))}
									onclick={() => launch(app)}
								>
									<img src={ctx.iconUrl(app.iconKey)} alt="" class="size-8" draggable="false" />
									<span class="w-full truncate text-center text-[11px] leading-tight">{app.name}</span>
								</button>
							</ContextMenuHost>
						{/each}
					</div>
					<Slot name="start.pinned" />
				</section>

				<section>
					<h2 class="mb-2 px-1 text-sm font-semibold">All apps</h2>
					{#each folders as [folder, list] (folder)}
						{#if folder}
							<div class="text-muted-foreground mt-3 mb-1 flex items-center gap-2 px-2.5 text-xs font-medium">
								<FolderOpen class="size-3.5" />
								{folder}
							</div>
						{/if}
						{#each list as app (app.id)}
							{@render appRow(app, 'l:' + app.id)}
						{/each}
					{/each}
					<Slot name="start.list" />
				</section>
			{/if}
		</div>

		<footer class="bg-background/40 relative flex items-center justify-between border-t px-4 py-2.5">
			<div class="flex items-center gap-2.5">
				<div class="bg-highlight/20 text-highlight flex size-8 items-center justify-center rounded-full text-sm font-semibold">
					A
				</div>
				<span class="text-sm">ARUI</span>
			</div>
			<div class="flex items-center gap-1">
				<Slot name="start.footer" />
				<button
					type="button"
					aria-label="ARUI settings"
					class="text-muted-foreground hover:bg-muted hover:text-foreground rounded-md p-2"
					onclick={() => {
						void ctx.surfaces.open('settings');
						void ctx.surface.close();
					}}
				>
					<Settings size={18} />
				</button>
				<button
					type="button"
					aria-label="Power"
					aria-expanded={powerOpen}
					class="text-muted-foreground hover:bg-muted hover:text-foreground rounded-md p-2 {powerOpen ? 'bg-muted text-foreground' : ''}"
					onclick={() => (powerOpen = !powerOpen)}
				>
					<Power size={18} />
				</button>
			</div>
			{#if powerOpen}
				<div class="arui-surface absolute right-4 bottom-14 z-10 w-48 p-1 text-sm">
					{#each powerItems as item, i (item.id)}
						{#if i === 3}<div class="bg-border -mx-1 my-1 h-px"></div>{/if}
						<button
							type="button"
							class="hover:bg-accent flex w-full items-center gap-2.5 rounded-md px-2 py-1.5"
							onclick={() => {
								powerOpen = false;
								void item.run();
							}}
						>
							<item.icon class="text-muted-foreground size-4" />
							{item.label}
						</button>
					{/each}
				</div>
			{/if}
		</footer>
	</div>
</div>
