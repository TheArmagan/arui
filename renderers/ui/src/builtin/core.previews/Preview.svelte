<script lang="ts">
	// 'preview' popup: one card per window of a group, each with a live DWM thumbnail drawn by
	// the host into the box rect (actions.showPreview).
	import type { AruiContext, WindowInfo } from '@arui/sdk';
	import X from '@lucide/svelte/icons/x';
	import { tick } from 'svelte';

	let { ctx }: { ctx: AruiContext } = $props();

	const CARD_W = 228;
	const THUMB_H = 128;
	const appId = $derived(String(ctx.surface.params.appId ?? ''));
	const group = $derived(
		ctx.state.groups.find((g) => g.appId.toLowerCase() === appId.toLowerCase()) ?? null
	);
	const windows = $derived(group?.windows ?? []);
	const boxes = new Map<number, HTMLElement>();

	function box(node: HTMLElement, hwnd: number) {
		boxes.set(hwnd, node);
		return {
			destroy() {
				boxes.delete(hwnd);
				void ctx.actions.hidePreview(hwnd);
			}
		};
	}

	// Size the window to fit the cards.
	$effect(() => {
		const n = Math.max(1, windows.length);
		const width = windows.length ? n * CARD_W + (n - 1) * 8 + 16 : 240;
		const height = windows.length ? THUMB_H + 58 : 76;
		void ctx.surface.resize({ width, height });
	});

	// Ask the host to draw thumbnails once the boxes are laid out (and again when they move).
	$effect(() => {
		const visible = ctx.surface.visible;
		const list = windows.map((w) => w.hwnd);
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
				void ctx.actions.showPreview(hwnd, {
					left: Math.round(r.left),
					top: Math.round(r.top),
					right: Math.round(r.right),
					bottom: Math.round(r.bottom)
				});
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

	function pointer(inside: boolean) {
		void ctx.events.emit('core.previews:pointer', { inside, appId });
	}

	function focus(w: WindowInfo) {
		void ctx.actions.focusWindow(w.hwnd);
		void ctx.surface.close();
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="h-full p-px"
	onmouseenter={() => pointer(true)}
	onmouseleave={() => pointer(false)}
>
	<div class="arui-surface flex h-full gap-2 p-2">
		{#if group && windows.length}
			{#each windows as w (w.hwnd)}
				<div
					class="group/card hover:bg-muted/60 flex shrink-0 flex-col gap-1.5 rounded-lg p-1.5 transition-colors"
					style:width="{CARD_W - 12}px"
				>
					<div class="flex items-center gap-2">
						<img src={ctx.iconUrl(w.iconKey || group.iconKey)} alt="" class="size-4" />
						<span class="min-w-0 flex-1 truncate text-xs font-medium">{w.title}</span>
						<button
							type="button"
							aria-label="Close {w.title}"
							class="text-muted-foreground hover:bg-destructive/80 rounded p-0.5 opacity-0 transition-opacity group-hover/card:opacity-100 hover:text-white"
							onclick={() => ctx.actions.closeWindow(w.hwnd)}
						>
							<X size={14} />
						</button>
					</div>
					<button
						type="button"
						aria-label="Switch to {w.title}"
						class="bg-background/60 relative w-full cursor-pointer overflow-hidden rounded-md"
						style:height="{THUMB_H}px"
						onclick={() => focus(w)}
					>
						<div use:box={w.hwnd} class="absolute inset-0"></div>
						{#if w.isMinimized}
							<span
								class="text-muted-foreground absolute right-1.5 bottom-1.5 z-10 rounded bg-black/50 px-1.5 text-[10px]"
								>Minimized</span
							>
						{/if}
					</button>
				</div>
			{/each}
		{:else if group}
			<div class="flex items-center gap-3 px-2">
				<img src={ctx.iconUrl(group.iconKey)} alt="" class="size-8" />
				<div class="min-w-0">
					<div class="truncate text-sm font-medium">{group.name}</div>
					<div class="text-muted-foreground text-xs">Pinned, not running</div>
				</div>
			</div>
		{/if}
	</div>
</div>
