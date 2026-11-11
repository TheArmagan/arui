<script lang="ts">
	// The legacy media card: blurred, darkened artwork behind title/artist and transport buttons,
	// with a thin progress line along the bottom edge.
	import type { AruiContext } from '@arui/sdk';
	import Music from '@lucide/svelte/icons/music';
	import Pause from '@lucide/svelte/icons/pause';
	import Play from '@lucide/svelte/icons/play';
	import SkipBack from '@lucide/svelte/icons/skip-back';
	import SkipForward from '@lucide/svelte/icons/skip-forward';

	let { ctx, orientation = 'horizontal' }: { ctx: AruiContext; orientation?: string } = $props();

	const media = $derived(ctx.state.currentMedia);
	const art = $derived(media?.artworkKey ? ctx.iconUrl(media.artworkKey) : '');
	const playing = $derived(media?.status === 'playing');
	const progress = $derived.by(() => {
		if (!media?.durationMs || media.positionMs == null) return 0;
		let pos = media.positionMs;
		if (playing && media.positionAtMs) pos += ctx.state.now.getTime() - media.positionAtMs;
		return Math.max(0, Math.min(1, pos / media.durationMs));
	});
	const hideWhenIdle = $derived(ctx.settings.get<boolean>('hideWhenIdle'));
</script>

{#if media || !hideWhenIdle}
	{#if orientation === 'vertical'}
		<button
			type="button"
			aria-label={playing ? 'Pause' : 'Play'}
			class="relative size-11 overflow-hidden rounded-lg border bg-cover bg-center"
			style:background-image={art ? `url("${art}")` : undefined}
			onclick={() => ctx.actions.media('togglePlayPause')}
		>
			<span class="absolute inset-0 flex items-center justify-center bg-black/45 text-white">
				{#if playing}<Pause size={18} />{:else}<Play size={18} />{/if}
			</span>
		</button>
	{:else}
		<div
			class="relative flex h-12 w-64 shrink-0 overflow-hidden rounded-lg bg-cover bg-center drop-shadow-[0_0_4px_rgba(0,0,0,0.5)] select-none"
			style:background-image={art ? `url("${art}")` : undefined}
		>
			<div
				class="flex w-full items-center justify-between gap-1 p-1 backdrop-blur-xs backdrop-brightness-50
					{art ? '' : 'bg-muted/60'}"
			>
				<div class="flex min-w-0 flex-1 items-center gap-2">
					<div
						class="bg-muted flex size-10 min-w-10 items-center justify-center rounded-md bg-cover bg-center shadow"
						style:background-image={art ? `url("${art}")` : undefined}
					>
						{#if !art}<Music size={16} class="text-muted-foreground" />{/if}
					</div>
					<div class="flex min-w-0 flex-col gap-1 drop-shadow-[0_0_4px_rgba(0,0,0,0.5)]">
						<span class="truncate text-sm leading-none font-semibold text-white">
							{media?.title || 'No media playing'}
						</span>
						<span class="truncate text-xs leading-none text-white/65">
							{media?.artist || 'Nothing in the queue'}
						</span>
					</div>
				</div>
				<div class="flex h-10 w-[68px] min-w-[68px] items-center justify-between pr-1 text-white/70">
					<button
						type="button"
						aria-label="Previous"
						disabled={!media?.canPrevious}
						class="cursor-pointer transition-colors duration-300 hover:text-white disabled:opacity-40"
						onclick={() => ctx.actions.media('previous')}
					>
						<SkipBack size={18} />
					</button>
					<button
						type="button"
						aria-label={playing ? 'Pause' : 'Play'}
						disabled={!media}
						class="cursor-pointer transition-colors duration-300 hover:text-white disabled:opacity-40"
						onclick={() => ctx.actions.media('togglePlayPause')}
					>
						{#if playing}<Pause size={18} />{:else}<Play size={18} />{/if}
					</button>
					<button
						type="button"
						aria-label="Next"
						disabled={!media?.canNext}
						class="cursor-pointer transition-colors duration-300 hover:text-white disabled:opacity-40"
						onclick={() => ctx.actions.media('next')}
					>
						<SkipForward size={18} />
					</button>
				</div>
			</div>
			{#if media?.durationMs}
				<div class="absolute right-0 bottom-0 left-0 h-[2px] bg-white/15">
					<div
						class="h-full bg-white/85 transition-[width] duration-1000 ease-linear"
						style:width="{progress * 100}%"
					></div>
				</div>
			{/if}
		</div>
	{/if}
{/if}
