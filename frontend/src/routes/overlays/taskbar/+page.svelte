<script lang="ts">
	import { api } from '@/base/api';
	import MouseEventsCapturer from '@/components/mouse-events-capturer.svelte';
	import { onMount } from 'svelte';
	import * as Card from '$lib/components/ui/card/index.js';
	import { Settings, X } from 'lucide-svelte';
	import * as Tooltip from '$lib/components/ui/tooltip/index.js';

	let shouldShowTaskbar = $state(false);
	let isHovering = $state(false);
	let now = $state(new Date());

	let ignoreHideOnce = $state(false);
	let hoverTimeout: ReturnType<typeof setTimeout> | null = null;

	// Helper functions for hover management
	function setHovering(value: boolean) {
		if (hoverTimeout) {
			clearTimeout(hoverTimeout);
			hoverTimeout = null;
		}
		isHovering = value;
	}

	function setHoveringWithDelay(value: boolean, delay: number) {
		if (hoverTimeout) {
			clearTimeout(hoverTimeout);
			hoverTimeout = null;
		}
		if (value) {
			isHovering = true;
		} else {
			hoverTimeout = setTimeout(() => {
				isHovering = false;
			}, delay);
		}
	}

	onMount(() => {
		let screens = api.ipc.getScreens();
		return api.events.on('KeyListenerMessage', (e) => {
			if (e.mode !== 'mouse' || e.data.event_type !== 'move') return;
			const { x, y } = e.data;
			let resetIgnore = false;
			screens.forEach((screen) => {
				if (y >= screen.bounds.height - 75) {
					shouldShowTaskbar = true;
				} else {
					if (!ignoreHideOnce) {
						resetIgnore = true;
						return;
					}
					if (!isHovering) {
						shouldShowTaskbar = false;
						document.querySelectorAll('.hide-when-taskbar-hidden').forEach((el) => {
							el.parentElement!.remove();
						});
					}
				}
			});
			if (resetIgnore) ignoreHideOnce = false;
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
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="bg-accent/85 flex items-center gap-4 rounded-lg border p-4"
			onmouseenter={() => setHovering(true)}
			onmouseleave={() => {
				setHoveringWithDelay(false, 100);
				ignoreHideOnce = true;
			}}
		>
			{#each api.native.taskbarItemList.taskbarItemsGrouped as group}
				{@const icon = api.native.taskbarItemList.icons[group[0].executable_path]}
				{@const isFocused = group.some((item) => item.is_focused)}
				{@const isRunning = group.some((item) => item.is_running)}
				<Tooltip.Provider delayDuration={100} disableCloseOnTriggerClick={true}>
					<Tooltip.Root>
						<Tooltip.Trigger>
							<button
								class="{isFocused
									? '-translate-y-1 opacity-100'
									: 'opacity-50'} transition-all duration-300 {isRunning
									? 'drop-shadow-[0_6px_6px_rgba(0,0,0,0.5)]'
									: ''}"
								onclick={() => {
									if (isRunning) {
										let hwnd = group.find((i) => i.hwnd)?.hwnd;
										if (!hwnd) return;
										api.native.taskbarItemList.toggleFocusWindow(hwnd);
									} else {
										api.native.taskbarItemList.startExecutable(group[0].executable_path);
									}
								}}
								onmouseenter={() => {
									if (isRunning) {
										group.forEach((item) => {
											api.native.taskbarItemList.getWindowScreenshot(item.hwnd, true);
										});
									}
								}}
							>
								<img
									src={`data:image/png;base64,${icon}`}
									class="h-8 w-8 cursor-pointer transition-all duration-300 hover:scale-110"
									alt={group[0].title}
								/>
							</button>
						</Tooltip.Trigger>
						<Tooltip.Content
							arrowClasses="hidden"
							class="bg-accent hide-when-taskbar-hidden p-0 text-white"
							sideOffset={24}
							side="top"
						>
							<!-- svelte-ignore a11y_mouse_events_have_key_events -->
							<MouseEventsCapturer
								class="flex gap-4 p-4"
								overlayId="taskbar"
								onMouseEvent={(e) => {
									if (e.type === 'leave') {
										e.preventDefault();
										setHoveringWithDelay(false, 100);
										setTimeout(() => {
											e.doDefault();
											shouldShowTaskbar = false;
										}, 100);
									} else if (e.type === 'enter') {
										setHovering(true);
									}
								}}
							>
								{#each group as item}
									{@const screenshot = api.native.taskbarItemList.screenshots[item.hwnd]?.data}
									{@const icon = api.native.taskbarItemList.icons[item.executable_path]}
									<!-- svelte-ignore a11y_click_events_have_key_events -->
									<div class="rounded-lg opacity-90 transition-all duration-300 hover:opacity-100">
										<div class="mb-2 flex items-center justify-between gap-2">
											<div class="flex items-center gap-2">
												<img
													src={`data:image/png;base64,${icon}`}
													class="h-6 w-6"
													alt={item.title}
												/>
												<span class="max-w-48 truncate text-sm font-semibold">{item.title}</span>
											</div>
											<div class="flex items-center gap-2">
												<!-- svelte-ignore a11y_click_events_have_key_events -->
												<div
													class="text-muted-foreground cursor-pointer hover:text-white"
													onclick={() => {
														api.native.taskbarItemList.closeWindow(item.hwnd);
													}}
												>
													<X size={16} />
												</div>
											</div>
										</div>
										<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
										<img
											src={`data:image/png;base64,${screenshot}`}
											class="cursor-pointer rounded"
											alt={group[0].title}
											draggable="false"
											onclick={() => {
												api.native.taskbarItemList.toggleFocusWindow(item.hwnd);
												shouldShowTaskbar = false;
												setHovering(false);
											}}
										/>
									</div>
								{/each}
							</MouseEventsCapturer>
						</Tooltip.Content>
					</Tooltip.Root>
				</Tooltip.Provider>
			{/each}
		</div>
	</MouseEventsCapturer>
</div>
