<script lang="ts">
	import { api } from '@/base/api';
	import { onDestroy, onMount, type Snippet } from 'svelte';

	const {
		children,
		overlayId,
		class: className = '',
		onMouseEvent
	}: {
		children: Snippet<[]>;
		overlayId: string;
		class?: string;
		onMouseEvent?: (o: {
			type: 'enter' | 'leave';
			preventDefault(): void;
			target: HTMLDivElement;
		}) => void;
	} = $props();

	let container = $state<HTMLDivElement | null>(null);
	let isMouseInside = $state(false);

	function handleMouseEnter() {
		let preventDefault = false;
		onMouseEvent?.({
			type: 'enter',
			preventDefault: () => (preventDefault = true),
			target: container!
		});
		if (!preventDefault) {
			api.ipc.setOverlayWindowIgnoreMouseEvents(overlayId, false);
		}
	}

	function handleMouseLeave() {
		let preventDefault = false;
		onMouseEvent?.({
			type: 'leave',
			preventDefault: () => (preventDefault = true),
			target: container!
		});
		if (!preventDefault) {
			api.ipc.setOverlayWindowIgnoreMouseEvents(overlayId, true);
		}
	}

	onMount(() => {
		return api.events.on('KeyListenerMessage', (e) => {
			if (!container) return;
			if (e.mode !== 'mouse' || e.data.event_type !== 'move') return;
			const rect = container.getBoundingClientRect();
			// check if the mouse is within the bounds of the container
			const isInside =
				e.data.x >= rect.left &&
				e.data.x <= rect.right &&
				e.data.y >= rect.top &&
				e.data.y <= rect.bottom;
			// update the state only if it has changed
			if (isMouseInside !== isInside) {
				isMouseInside = isInside;
				if (isMouseInside) {
					handleMouseEnter();
				} else {
					handleMouseLeave();
				}
			}
		});
	});

	onDestroy(() => {
		if (isMouseInside) {
			handleMouseLeave();
		}
	});
</script>

<div bind:this={container} class={className}>
	{@render children()}
</div>
