<script lang="ts">
	// tray.inline widget: the icons that stay visible on the taskbar.
	import type { AruiContext } from '@arui/sdk';
	import { getUiHost } from '@arui/sdk/ui';
	import { isInline, trayHandlers } from './tray';

	let { ctx, orientation = 'horizontal' }: { ctx: AruiContext; orientation?: string } = $props();
	const host = getUiHost();
	const icons = $derived(
		ctx.state.tray.filter((t) => t.visible).filter((t, i) => isInline(ctx, t, i))
	);
</script>

<div class="flex items-center gap-0.5 {orientation === 'vertical' ? 'flex-col' : ''}">
	{#each icons as icon (icon.id)}
		<button
			type="button"
			title={icon.tooltip}
			aria-label={icon.tooltip}
			class="hover:bg-muted/70 flex size-8 items-center justify-center rounded-md transition-colors"
			{...trayHandlers(ctx, host, icon, true)}
		>
			<img src={ctx.iconUrl(icon.iconKey)} alt="" class="size-[18px]" draggable="false" />
		</button>
	{/each}
</div>
