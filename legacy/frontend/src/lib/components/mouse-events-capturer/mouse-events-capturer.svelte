<script lang="ts">
	import { api } from '@/base/api';
	import { onDestroy, onMount, type Snippet } from 'svelte';
	import { mouseEventsCapturer } from './mouse-events-capturer-store.svelte';

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
			type: 'enter' | 'leave' | 'move';
			preventDefault(): void;
			doDefault(): void;
			target: HTMLDivElement;
		}) => void;
	} = $props();

	let container = $state<HTMLDivElement | null>(null);
	const instanceId = crypto.randomUUID();

	onMount(() => {
		if (!container) return;

		const unregister = mouseEventsCapturer.register({
			id: instanceId,
			overlayId,
			isMouseInside: false,
			container,
			onMouseEvent
		});

		const unsubscribe = api.events.on('KeyListenerMessage', (e) => {
			if (!container) return;
			if (e.mode !== 'mouse' || e.data.event_type !== 'move') return;

			const rect = container.getBoundingClientRect();
			const isInside =
				e.data.x >= rect.left &&
				e.data.x <= rect.right &&
				e.data.y >= rect.top &&
				e.data.y <= rect.bottom;

			mouseEventsCapturer.updateMousePosition(instanceId, isInside);

			if (isInside) {
				mouseEventsCapturer.handleMove(instanceId);
			}
		});

		return () => {
			unregister();
			unsubscribe();
		};
	});

	onDestroy(() => {
		mouseEventsCapturer.unregister(instanceId);
	});
</script>

<div bind:this={container} class={className}>
	{@render children()}
</div>
