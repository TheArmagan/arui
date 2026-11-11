<script lang="ts">
	import type { AruiContext } from '@arui/sdk';
	import Pause from '@lucide/svelte/icons/pause';
	import Play from '@lucide/svelte/icons/play';
	import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
	import { durationMs, read, remaining, reset, toggle } from './timer';

	let { ctx, orientation = 'horizontal' }: { ctx: AruiContext; orientation?: string } = $props();

	const state = $derived(read(ctx));
	const total = $derived(state.status === 'idle' ? durationMs(ctx) : state.totalMs);
	const left = $derived(remaining(state, ctx.state.now.getTime(), total));
	const done = $derived(state.status === 'running' && left === 0);
	const showSeconds = $derived(ctx.settings.get<boolean>('showSeconds'));
	const label = $derived.by(() => {
		const secs = Math.ceil(left / 1000);
		const m = Math.floor(secs / 60);
		const s = secs % 60;
		return showSeconds ? `${m}:${String(s).padStart(2, '0')}` : `${Math.ceil(left / 60000)}m`;
	});
	// Ring progress: 0 = full time left, 1 = finished.
	const progress = $derived(total > 0 ? 1 - left / total : 0);
	const R = 13;
	const C = 2 * Math.PI * R;
</script>

<button
	type="button"
	class="timer"
	class:vertical={orientation === 'vertical'}
	class:running={state.status === 'running' && !done}
	class:done
	title={done ? 'Focus session finished. Click to reset.' : 'Focus timer: click to start or pause'}
	onclick={() => (done ? reset(ctx) : toggle(ctx))}
>
	<svg viewBox="0 0 32 32" class="ring" aria-hidden="true">
		<circle cx="16" cy="16" r={R} class="track" />
		<circle
			cx="16"
			cy="16"
			r={R}
			class="bar"
			stroke-dasharray={C}
			stroke-dashoffset={C * progress}
			transform="rotate(-90 16 16)"
		/>
	</svg>
	<span class="icon">
		{#if done}<RotateCcw size={12} />{:else if state.status === 'running'}<Pause size={12} />{:else}<Play
				size={12}
			/>{/if}
	</span>
	{#if orientation !== 'vertical'}
		<span class="text">
			<span class="time">{label}</span>
			<span class="caption">{done ? 'Break time' : state.status === 'paused' ? 'Paused' : 'Focus'}</span>
		</span>
	{/if}
</button>

<style>
	.timer {
		position: relative;
		display: flex;
		align-items: center;
		gap: 8px;
		height: 44px;
		padding: 0 12px 0 6px;
		border-radius: calc(var(--arui-radius) + 2px);
		border: 1px solid var(--arui-border);
		background: color-mix(in oklab, var(--arui-card) 70%, transparent);
		color: var(--arui-card-fg);
		cursor: pointer;
		transition: background 0.2s;
	}
	.timer:hover {
		background: var(--arui-muted);
	}
	.timer.vertical {
		padding: 6px;
	}
	.ring {
		width: 32px;
		height: 32px;
	}
	.track,
	.bar {
		fill: none;
		stroke-width: 3;
	}
	.track {
		stroke: color-mix(in oklab, var(--arui-fg) 14%, transparent);
	}
	.bar {
		stroke: var(--arui-highlight);
		stroke-linecap: round;
		transition: stroke-dashoffset 1s linear;
	}
	.icon {
		position: absolute;
		left: 16px;
		top: 50%;
		transform: translateY(-50%);
		display: flex;
		color: var(--arui-muted-fg);
	}
	.vertical .icon {
		left: 16px;
	}
	.running .icon {
		color: var(--arui-highlight);
	}
	.text {
		display: flex;
		flex-direction: column;
		line-height: 1.15;
		text-align: left;
	}
	.time {
		font-size: 14px;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}
	.caption {
		font-size: 11px;
		color: var(--arui-muted-fg);
	}
	.done {
		animation: pulse 1.6s ease-in-out infinite;
		border-color: var(--arui-highlight);
	}
	@keyframes pulse {
		50% {
			background: color-mix(in oklab, var(--arui-highlight) 22%, transparent);
		}
	}
</style>
