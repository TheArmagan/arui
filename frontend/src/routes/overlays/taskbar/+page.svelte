<script>
	import { api } from '@/base/api';
	import MouseEventsCapturer from '@/components/mouse-events-capturer.svelte';
	import { onMount } from 'svelte';
	import * as Card from '$lib/components/ui/card/index.js';
	import { Settings } from 'lucide-svelte';

	let shouldShowTaskbar = $state(false);
	let now = $state(new Date());

	onMount(() => {
		let screens = api.ipc.getScreens();
		return api.events.on('KeyListenerMessage', (e) => {
			if (e.mode !== 'mouse' || e.data.event_type !== 'move') return;
			const { x, y } = e.data;
			screens.forEach((screen) => {
				if (y >= screen.bounds.height - 100) {
					shouldShowTaskbar = true;
				} else {
					shouldShowTaskbar = false;
				}
			});
		});
	});

	onMount(() => {
		const interval = setInterval(() => {
			now = new Date();
		}, 1000);
		return () => clearInterval(interval);
	});
</script>

<div class="relative flex h-[100vh] w-full items-end justify-center p-4 contain-content">
	<MouseEventsCapturer
		overlayId="taskbar"
		class="bg-accent/85 absolute right-2 top-2 transform rounded-lg border px-4 py-2 text-sm font-semibold text-white shadow transition-all duration-300"
		onMouseEvent={(e) => {
			e.preventDefault();
			if (e.type === 'enter') {
				e.target.classList.add('opacity-25');
			} else if (e.type === 'leave') {
				e.target.classList.remove('opacity-25');
			}
		}}
	>
		{now.toLocaleString('en-US', {
			weekday: 'short',
			year: 'numeric',
			month: 'short',
			day: 'numeric',
			hour: '2-digit',
			minute: '2-digit',
			second: '2-digit',
			hour12: true
		})}
	</MouseEventsCapturer>

	<MouseEventsCapturer
		overlayId="taskbar"
		class="{shouldShowTaskbar
			? 'translate-y-0'
			: 'translate-y-32'} w-full drop-shadow-[0_8px_12px_rgba(0,0,0,0.5)] transition-all duration-300"
	>
		<Card.Root class="bg-accent/85 flex w-full items-start justify-center gap-4 p-4">
			<div class="border-r pr-2">
				<Settings
					class="h-8 w-8 cursor-pointer transition-all duration-300 hover:rotate-180 hover:scale-110"
				/>
			</div>
		</Card.Root>
	</MouseEventsCapturer>
</div>
