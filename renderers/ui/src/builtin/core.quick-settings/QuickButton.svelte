<script lang="ts">
	// taskbar.end widget: network + volume status; click opens quick settings, wheel adjusts volume.
	import type { AruiContext } from '@arui/sdk';
	import { getUiHost } from '@arui/sdk/ui';
	import { networkIcon, networkLabel, volumeIcon } from './icons';

	let { ctx, orientation = 'horizontal' }: { ctx: AruiContext; orientation?: string } = $props();
	const host = getUiHost();
	let el = $state<HTMLButtonElement>();
	const VolumeIcon = $derived(volumeIcon(ctx.state.volume));
	const NetIcon = $derived(networkIcon(ctx.state.network));

	function onWheel(e: WheelEvent) {
		e.preventDefault();
		const step = e.deltaY < 0 ? 0.02 : -0.02;
		void ctx.actions.setVolume(Math.round((ctx.state.volume.level + step) * 100) / 100);
	}
</script>

<button
	bind:this={el}
	type="button"
	title="{networkLabel(ctx.state.network)} - Volume {Math.round(ctx.state.volume.level * 100)}%"
	class="text-foreground/85 hover:bg-muted/70 flex items-center gap-2.5 rounded-lg px-2.5 py-2 transition-colors
		{orientation === 'vertical' ? 'flex-col' : ''}"
	onwheel={onWheel}
	onclick={() =>
		ctx.surfaces.toggle({
			id: 'quick-settings',
			monitor: ctx.surface.monitor ?? undefined,
			rect: el ? host.screenRect(el.getBoundingClientRect()) : undefined
		})}
>
	<NetIcon size={17} />
	<VolumeIcon size={17} />
</button>
