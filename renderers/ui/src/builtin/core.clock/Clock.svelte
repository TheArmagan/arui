<script lang="ts">
	import type { AruiContext } from '@arui/sdk';
	import { getUiHost } from '@arui/sdk/ui';

	let { ctx, orientation = 'horizontal' }: { ctx: AruiContext; orientation?: string } = $props();
	const host = getUiHost();
	let el = $state<HTMLButtonElement>();

	const showSeconds = $derived(ctx.settings.get<boolean>('showSeconds'));
	const hour12 = $derived(ctx.settings.get<boolean>('hour12'));
	const vertical = $derived(orientation === 'vertical');
	// Vertical bars are narrow: hours and minutes only, 24-hour.
	const time = $derived(
		ctx.state.now.toLocaleTimeString('en-US', {
			hour: '2-digit',
			minute: '2-digit',
			second: showSeconds && !vertical ? '2-digit' : undefined,
			hour12: vertical ? false : hour12
		})
	);
	const date = $derived(
		ctx.state.now.toLocaleDateString('en-US', {
			weekday: vertical ? undefined : 'short',
			month: 'short',
			day: 'numeric',
			year: vertical ? undefined : 'numeric'
		})
	);
</script>

<button
	bind:this={el}
	type="button"
	class="arui-bar-button flex h-9 flex-col justify-center px-2 text-right
		{orientation === 'vertical' ? 'items-center text-center' : 'items-end'}"
	onclick={() =>
		ctx.surfaces.toggle({
			id: 'calendar',
			monitor: ctx.surface.monitor ?? undefined,
			rect: el ? host.screenRect(el.getBoundingClientRect()) : undefined
		})}
>
	<span class="text-foreground text-xs leading-tight font-semibold tabular-nums">{time}</span>
	<span class="text-muted-foreground text-[11px] leading-tight whitespace-nowrap">{date}</span>
</button>
