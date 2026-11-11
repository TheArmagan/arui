<script lang="ts">
	// 'tray-flyout' popup: overflow icons in a grid plus the tray.flyout slot.
	import type { AruiContext } from '@arui/sdk';
	import { Slot, getUiHost } from '@arui/sdk/ui';
	import { isInline, trayHandlers } from './tray';

	let { ctx }: { ctx: AruiContext } = $props();
	const host = getUiHost();
	const icons = $derived(
		ctx.state.tray.filter((t) => t.visible).filter((t, i) => !isInline(ctx, t, i))
	);
	let height = $state(0);
	$effect(() => {
		if (height > 0) void ctx.surface.resize({ height: Math.ceil(height) + 2 });
	});
</script>

<div class="p-px">
	<div bind:clientHeight={height} class="arui-surface flex flex-col gap-1.5 p-1.5">
		{#if icons.length}
			<div class="grid grid-cols-4 gap-0.5">
				{#each icons as icon (icon.id)}
					<button
						type="button"
						title={icon.tooltip}
						aria-label={icon.tooltip}
						class="arui-bar-button flex items-center justify-center size-9"
						{...trayHandlers(ctx, host, icon, false)}
					>
						<img src={ctx.iconUrl(icon.iconKey)} alt="" class="size-4" draggable="false" />
					</button>
				{/each}
			</div>
		{:else}
			<div class="text-muted-foreground px-2 py-3 text-center text-xs">No hidden icons</div>
		{/if}
		<Slot name="tray.flyout" />
		<div class="text-muted-foreground/70 border-t px-1 pt-1.5 text-[10px]">
			Shift + right click an icon to move it
		</div>
	</div>
</div>
