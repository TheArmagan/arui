<script lang="ts">
	// taskbar.end widget: overflow chevron plus the tray.inline slot.
	import type { AruiContext } from '@arui/sdk';
	import { Slot, getUiHost } from '@arui/sdk/ui';
	import ChevronUp from '@lucide/svelte/icons/chevron-up';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';

	let { ctx, orientation = 'horizontal' }: { ctx: AruiContext; orientation?: string } = $props();
	const host = getUiHost();
	let el = $state<HTMLButtonElement>();
	const vertical = $derived(orientation === 'vertical');
</script>

<div class="flex items-center gap-0.5 {vertical ? 'flex-col' : ''}">
	<button
		bind:this={el}
		type="button"
		aria-label="Show hidden icons"
		class="arui-bar-button flex items-center justify-center text-muted-foreground hover:text-foreground size-7"
		onclick={() =>
			ctx.surfaces.toggle({
				id: 'tray-flyout',
				monitor: ctx.surface.monitor ?? undefined,
				rect: el ? host.screenRect(el.getBoundingClientRect()) : undefined
			})}
	>
		{#if vertical}<ChevronRight size={14} />{:else}<ChevronUp size={14} />{/if}
	</button>
	<Slot name="tray.inline" props={{ orientation }} />
</div>
