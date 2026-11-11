<script lang="ts">
	// 'preview' popup: one card per window of a group, each with a live DWM thumbnail drawn by
	// the host into the box rect (actions.showPreview). The popup measures its own content and
	// resizes the window to fit it exactly.
	import type { AruiContext, WindowInfo } from '@arui/sdk';
	import X from '@lucide/svelte/icons/x';
	import { tick } from 'svelte';

	let { ctx }: { ctx: AruiContext } = $props();

	const appId = $derived(String(ctx.surface.params.appId ?? ''));
	const group = $derived(
		ctx.state.groups.find((g) => g.appId.toLowerCase() === appId.toLowerCase()) ?? null
	);
	const windows = $derived(group?.windows ?? []);
	const boxes = new Map<number, HTMLElement>();

	// Each thumbnail box takes its window's aspect ratio (reported by showPreview), so the live
	// thumbnail fills it with no letterboxing. Boxes share one height, which shrinks (down to
	// 72px) so many windows still fit on the screen. Until a ratio is known, 16:10 is assumed.
	const DEFAULT_RATIO = 16 / 10;
	const MAX_H = 124;
	const MIN_H = 72;
	const MIN_CARD_W = 120;
	let ratios = $state<Record<number, number>>({});
	const ratioOf = (hwnd: number) => ratios[hwnd] ?? DEFAULT_RATIO;
	const thumbH = $derived.by(() => {
		if (!windows.length) return MAX_H;
		// Card chrome (padding, border, gap) is ~22px per card plus ~18px of outer padding.
		const avail = (window.screen?.availWidth ?? 1920) * 0.92 - 18 - windows.length * 22;
		const sum = windows.reduce((t, w) => t + ratioOf(w.hwnd), 0);
		return Math.round(Math.max(MIN_H, Math.min(MAX_H, avail / sum)));
	});
	const boxW = (hwnd: number) => Math.round(thumbH * ratioOf(hwnd));

	function setRatio(hwnd: number, size: { width: number; height: number } | undefined) {
		if (!size?.width || !size?.height) return;
		// Clamp extreme shapes (e.g. tool strips) so cards stay usable.
		const r = Math.min(2.4, Math.max(0.5, size.width / size.height));
		if (Math.abs((ratios[hwnd] ?? 0) - r) > 0.01) ratios = { ...ratios, [hwnd]: r };
	}

	function box(node: HTMLElement, hwnd: number) {
		boxes.set(hwnd, node);
		return {
			destroy() {
				boxes.delete(hwnd);
				void ctx.actions.hidePreview(hwnd);
			}
		};
	}

	// Fit the window to the measured content.
	let width = $state(0);
	let height = $state(0);
	$effect(() => {
		if (width > 0 && height > 0) {
			void ctx.surface.resize({ width: Math.ceil(width), height: Math.ceil(height) });
		}
	});

	// Ask the host to draw thumbnails once the boxes are laid out (and again when they move).
	$effect(() => {
		const visible = ctx.surface.visible;
		const list = windows.map((w) => w.hwnd);
		void thumbH;
		void ratios;
		if (!visible) {
			void ctx.actions.hidePreview();
			return;
		}
		let cancelled = false;
		const draw = async () => {
			await tick();
			await new Promise((r) => requestAnimationFrame(r));
			if (cancelled) return;
			for (const hwnd of list) {
				const el = boxes.get(hwnd);
				if (!el) continue;
				const r = el.getBoundingClientRect();
				ctx.actions
					.showPreview(hwnd, {
						left: Math.round(r.left),
						top: Math.round(r.top),
						right: Math.round(r.right),
						bottom: Math.round(r.bottom)
					})
					.then((size) => setRatio(hwnd, size))
					.catch(() => {});
			}
		};
		void draw();
		const ro = new ResizeObserver(() => void draw());
		ro.observe(document.body);
		return () => {
			cancelled = true;
			ro.disconnect();
		};
	});

	// Peek: holding the pointer on a card shows that window in front (without activating it).
	// Once a peek has happened, the popup is in peek mode: moving to another card switches the
	// peek instantly. Leaving a card restores after a short grace (so crossing the gap between
	// cards does not flicker); leaving the popup or closing it ends peek mode. Clicking keeps
	// the window.
	const LEAVE_GRACE_MS = 150;
	let peekTimer: ReturnType<typeof setTimeout> | undefined;
	let leaveTimer: ReturnType<typeof setTimeout> | undefined;
	let peeking: number | null = null;
	let peekMode = false;

	function doPeek(w: WindowInfo) {
		peekMode = true;
		if (peeking === w.hwnd) return;
		peeking = w.hwnd;
		void ctx.actions.peekWindow(w.hwnd).catch(() => {
			if (peeking === w.hwnd) peeking = null;
		});
	}

	function peekStart(w: WindowInfo) {
		clearTimeout(peekTimer);
		clearTimeout(leaveTimer);
		if (!ctx.settings.get<boolean>('peek')) return;
		if (peekMode) return doPeek(w);
		peekTimer = setTimeout(() => doPeek(w), ctx.settings.get<number>('peekDelay') ?? 600);
	}

	function cardLeave() {
		clearTimeout(peekTimer);
		if (peeking == null) return;
		clearTimeout(leaveTimer);
		leaveTimer = setTimeout(() => restorePeek(true), LEAVE_GRACE_MS);
	}

	function restorePeek(restore: boolean) {
		clearTimeout(leaveTimer);
		if (peeking == null) return;
		peeking = null;
		void ctx.actions.unpeekWindow(restore).catch(() => {});
	}

	/** Ends peeking and peek mode (popup left/hidden, window clicked or closed). */
	function peekEnd(restore = true) {
		clearTimeout(peekTimer);
		peekMode = false;
		restorePeek(restore);
	}

	$effect(() => {
		if (!ctx.surface.visible) peekEnd();
	});
	$effect(() => () => peekEnd());

	function pointer(inside: boolean) {
		if (!inside) peekEnd();
		void ctx.events.emit('core.previews:pointer', { inside, appId });
	}

	function focus(w: WindowInfo) {
		peekEnd(false);
		void ctx.actions.focusWindow(w.hwnd);
		void ctx.surface.close();
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	bind:clientWidth={width}
	bind:clientHeight={height}
	class="w-max p-px"
	onmouseenter={() => pointer(true)}
	onmouseleave={() => pointer(false)}
>
	<!-- No panel behind the cards: each card carries its own chrome. Padding leaves room for shadows. -->
	<div class="flex gap-2 p-2">
		{#if group && windows.length}
			{#each windows as w (w.hwnd)}
				<div
					class="group/card bg-popover/95 text-popover-foreground rounded-xl border shadow-[0_6px_16px_-6px_rgb(0_0_0/0.6),inset_0_1px_0_rgb(255_255_255/0.06)] flex shrink-0 flex-col gap-1.5 p-1.5 transition-[background-color,border-color] duration-150 ease-out
						hover:bg-card hover:border-white/15"
					onmouseenter={() => peekStart(w)}
					onmouseleave={cardLeave}
				>
					<div
						class="flex h-5 items-center gap-1.5 pl-0.5"
						style:width="{Math.max(MIN_CARD_W, boxW(w.hwnd))}px"
					>
						<img
							src={ctx.iconUrl(w.iconKey || group.iconKey)}
							alt=""
							draggable="false"
							class="size-3.5 shrink-0"
						/>
						<span
							class="min-w-0 flex-1 truncate text-xs leading-none
								{w.isFocused ? 'text-foreground font-medium' : 'text-foreground/80'}">{w.title}</span
						>
						<button
							type="button"
							aria-label="Close {w.title}"
							class="text-muted-foreground flex size-5 shrink-0 items-center justify-center rounded-md opacity-0 transition-[opacity,background-color,color] duration-150
								group-hover/card:opacity-100 hover:bg-red-500/90 hover:text-white"
							onclick={() => {
								peekEnd(false);
								void ctx.actions.closeWindow(w.hwnd);
							}}
						>
							<X size={12} strokeWidth={2.5} />
						</button>
					</div>
					<button
						type="button"
						aria-label="Switch to {w.title}"
						class="relative cursor-pointer self-center overflow-hidden rounded-md bg-black/30 shadow-[inset_0_0_0_1px_rgb(255_255_255/0.05)]
							{w.isFocused ? 'ring-highlight/50 ring-1' : ''}"
						style:width="{boxW(w.hwnd)}px"
						style:height="{thumbH}px"
						onclick={() => focus(w)}
					>
						<!-- Shown when the thumbnail is empty (e.g. minimized windows); DWM draws over it. -->
						<img
							src={ctx.iconUrl(w.iconKey || group.iconKey)}
							alt=""
							draggable="false"
							class="absolute top-1/2 left-1/2 size-8 -translate-x-1/2 -translate-y-1/2 opacity-40"
						/>
						<div use:box={w.hwnd} class="absolute inset-0"></div>
						{#if w.isMinimized}
							<span
								class="absolute right-1 bottom-1 z-10 rounded bg-black/60 px-1.5 py-px text-[10px] text-white/70"
								>Minimized</span
							>
						{/if}
					</button>
				</div>
			{/each}
		{:else if group}
			<div class="bg-popover/95 text-popover-foreground rounded-xl border shadow-[0_6px_16px_-6px_rgb(0_0_0/0.6),inset_0_1px_0_rgb(255_255_255/0.06)] flex items-center gap-2.5 py-1.5 pr-3 pl-2">
				<img src={ctx.iconUrl(group.iconKey)} alt="" draggable="false" class="size-6" />
				<div class="flex min-w-0 flex-col gap-0.5">
					<span class="max-w-48 truncate text-xs leading-tight font-medium">{group.name}</span>
					<span class="text-muted-foreground text-[11px] leading-tight">Not running</span>
				</div>
			</div>
		{/if}
	</div>
</div>
