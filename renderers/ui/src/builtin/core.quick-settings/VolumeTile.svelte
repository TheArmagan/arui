<script lang="ts">
	import type { AruiContext } from '@arui/sdk';
	import { volumeIcon } from './icons';

	let { ctx }: { ctx: AruiContext } = $props();
	const vol = $derived(ctx.state.volume);
	const Icon = $derived(volumeIcon(vol));
	const pct = $derived(Math.round(vol.level * 100));
</script>

<div class="flex items-center gap-3">
	<button
		type="button"
		aria-label={vol.muted ? 'Unmute' : 'Mute'}
		class="flex size-9 shrink-0 items-center justify-center rounded-full transition-colors
			{vol.muted ? 'bg-muted text-muted-foreground' : 'bg-highlight text-black'}"
		onclick={() => ctx.actions.setMuted(!vol.muted)}
	>
		<Icon size={17} />
	</button>
	<input
		type="range"
		min="0"
		max="100"
		value={pct}
		aria-label="Volume"
		class="volume-range h-1.5 flex-1"
		style:--pct="{pct}%"
		oninput={(e) => ctx.actions.setVolume(Number(e.currentTarget.value) / 100)}
	/>
	<span class="text-muted-foreground w-8 text-right text-xs tabular-nums">{pct}</span>
</div>

<style>
	.volume-range {
		appearance: none;
		border-radius: 999px;
		background: linear-gradient(
			to right,
			var(--arui-highlight) var(--pct),
			color-mix(in oklab, var(--arui-fg) 18%, transparent) var(--pct)
		);
		cursor: pointer;
	}
	.volume-range::-webkit-slider-thumb {
		appearance: none;
		width: 16px;
		height: 16px;
		border-radius: 999px;
		background: var(--arui-fg);
		box-shadow: 0 0 0 4px color-mix(in oklab, var(--arui-highlight) 35%, transparent);
	}
</style>
