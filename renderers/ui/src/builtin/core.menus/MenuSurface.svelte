<script lang="ts">
	// Popup surface that shows a context menu requested by another window (see
	// Runtime.popupMenu). Items arrive serialized in the surface params; picking one broadcasts
	// `core.menus:select` so the requesting window runs the item's callback.
	import type { AruiContext } from '@arui/sdk';
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import { tick } from 'svelte';

	interface WireItem {
		id: string;
		label: string;
		icon?: string;
		disabled?: boolean;
		separatorBefore?: boolean;
		children?: WireItem[];
	}

	let { ctx }: { ctx: AruiContext } = $props();

	const token = $derived(String(ctx.surface.params.token ?? ''));
	const root = $derived((ctx.surface.params.items as WireItem[] | undefined) ?? []);
	let trail = $state<WireItem[]>([]);
	let active = $state(0);
	let height = $state(0);
	let listEl = $state<HTMLElement>();

	const items = $derived(trail.length ? (trail[trail.length - 1].children ?? []) : root);

	// A new menu resets navigation.
	$effect(() => {
		void token;
		trail = [];
		active = 0;
		tick().then(() => listEl?.focus());
	});

	$effect(() => {
		if (height > 0) void ctx.surface.resize({ height: Math.ceil(height) + 2 });
	});

	function choose(item: WireItem) {
		if (item.disabled) return;
		if (item.children?.length) {
			trail = [...trail, item];
			active = 0;
			return;
		}
		const path = [...trail.map((t) => t.id), item.id];
		void ctx.events.emit('core.menus:select', { token, path });
		void ctx.surface.close();
	}

	function onKey(e: KeyboardEvent) {
		const enabled = items.map((it, i) => (it.disabled ? -1 : i)).filter((i) => i >= 0);
		if (!enabled.length) return;
		const pos = enabled.indexOf(active);
		if (e.key === 'ArrowDown') {
			active = enabled[(pos + 1) % enabled.length];
		} else if (e.key === 'ArrowUp') {
			active = enabled[(pos - 1 + enabled.length) % enabled.length];
		} else if (e.key === 'Enter' || e.key === 'ArrowRight') {
			const it = items[active];
			if (it && (e.key === 'Enter' || it.children?.length)) choose(it);
		} else if (e.key === 'ArrowLeft' || e.key === 'Backspace') {
			if (trail.length) trail = trail.slice(0, -1);
		} else if (e.key === 'Escape') {
			void ctx.surface.close();
		} else return;
		e.preventDefault();
	}
</script>

<div class="h-full p-px">
	<div
		bind:clientHeight={height}
		bind:this={listEl}
		role="menu"
		tabindex="-1"
		onkeydown={onKey}
		class="arui-surface text-popover-foreground p-1 text-sm outline-none"
	>
		{#if trail.length}
			<button
				type="button"
				class="text-muted-foreground hover:text-foreground flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs"
				onclick={() => (trail = trail.slice(0, -1))}
			>
				<ChevronLeft class="size-3.5" />
				{trail[trail.length - 1].label}
			</button>
			<div class="bg-border -mx-1 my-1 h-px"></div>
		{/if}
		{#each items as item, i (item.id)}
			{#if item.separatorBefore && i > 0}
				<div class="bg-border -mx-1 my-1 h-px"></div>
			{/if}
			<button
				type="button"
				role="menuitem"
				disabled={item.disabled}
				class="flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left transition-colors disabled:opacity-40
					{i === active ? 'bg-accent text-accent-foreground' : ''}"
				onmouseenter={() => (active = i)}
				onclick={() => choose(item)}
			>
				<span class="text-muted-foreground flex size-4 shrink-0 items-center justify-center [&_svg]:size-4">
					{#if item.icon?.startsWith('svg:')}
						{@html item.icon.slice(4)}
					{:else if item.icon}
						<img src={item.icon} alt="" class="size-4 object-contain" />
					{/if}
				</span>
				<span class="min-w-0 flex-1 truncate">{item.label}</span>
				{#if item.children?.length}
					<ChevronRight class="text-muted-foreground size-3.5" />
				{/if}
			</button>
		{/each}
	</div>
</div>
