<script lang="ts">
	// 'calendar' popup: current time and a month view.
	import type { AruiContext } from '@arui/sdk';
	import ChevronLeft from '@lucide/svelte/icons/chevron-left';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';

	let { ctx }: { ctx: AruiContext } = $props();

	let offset = $state(0);
	// Back to the current month every time the popup opens.
	$effect(() => {
		if (ctx.surface.visible) offset = 0;
	});

	const now = $derived(ctx.state.now);
	const view = $derived(new Date(now.getFullYear(), now.getMonth() + offset, 1));
	const weekStart = 1; // Monday
	const days = $derived.by(() => {
		const first = new Date(view);
		const lead = (first.getDay() - weekStart + 7) % 7;
		const start = new Date(first.getFullYear(), first.getMonth(), 1 - lead);
		return Array.from({ length: 42 }, (_, i) => new Date(start.getFullYear(), start.getMonth(), start.getDate() + i));
	});
	const names = Array.from({ length: 7 }, (_, i) =>
		new Date(2024, 0, 1 + i).toLocaleDateString('en-US', { weekday: 'narrow' })
	);
	const sameDay = (a: Date, b: Date) => a.toDateString() === b.toDateString();
</script>

<div class="arui-surface flex h-full flex-col gap-4 overflow-hidden p-5">
	<div>
		<div class="font-display text-4xl font-semibold tracking-tight tabular-nums">
			{now.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', second: '2-digit' })}
		</div>
		<div class="text-muted-foreground mt-1 text-sm">
			{now.toLocaleDateString('en-US', { weekday: 'long', month: 'long', day: 'numeric', year: 'numeric' })}
		</div>
	</div>
	<div class="bg-border h-px"></div>
	<div class="flex items-center justify-between">
		<div class="text-sm font-semibold">
			{view.toLocaleDateString('en-US', { month: 'long', year: 'numeric' })}
		</div>
		<div class="flex gap-1">
			<button
				type="button"
				aria-label="Previous month"
				class="hover:bg-muted text-muted-foreground hover:text-foreground rounded-md p-1.5"
				onclick={() => offset--}><ChevronLeft class="size-4" /></button
			>
			<button
				type="button"
				aria-label="Next month"
				class="hover:bg-muted text-muted-foreground hover:text-foreground rounded-md p-1.5"
				onclick={() => offset++}><ChevronRight class="size-4" /></button
			>
		</div>
	</div>
	<div class="grid grid-cols-7 gap-1 text-center text-xs">
		{#each names as n, i (i)}
			<div class="text-muted-foreground py-1 font-medium">{n}</div>
		{/each}
		{#each days as d (d.toISOString())}
			{@const inMonth = d.getMonth() === view.getMonth()}
			<div
				class="flex aspect-square items-center justify-center rounded-full tabular-nums
					{sameDay(d, now)
					? 'bg-highlight font-semibold text-black'
					: inMonth
						? 'hover:bg-muted'
						: 'text-muted-foreground/50'}"
			>
				{d.getDate()}
			</div>
		{/each}
	</div>
</div>
