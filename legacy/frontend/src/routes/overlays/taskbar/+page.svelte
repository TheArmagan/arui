<script lang="ts">
	import { api } from '@/base/api';
	import MouseEventsCapturer from '@/components/mouse-events-capturer/mouse-events-capturer.svelte';
	import { onMount } from 'svelte';
	import {
		LayoutList,
		LoaderCircle,
		Pause,
		Play,
		Settings,
		SkipBack,
		SkipForward,
		Terminal,
		TrainTrack,
		X
	} from 'lucide-svelte';
	import * as Tooltip from '$lib/components/ui/tooltip/index.js';
	import * as ContextMenu from '$lib/components/ui/context-menu/index.js';
	import gsap from 'gsap';
	import type { AnimationConfig } from 'svelte/animate';

	function smoothMove(
		node: HTMLElement,
		{ from, to }: { from: DOMRect; to: DOMRect },
		params: { duration?: number; delay?: number } = {}
	): AnimationConfig {
		const tl = gsap.timeline({
			defaults: {
				duration: params.duration || 0.3
			},
			onComplete: () => {
				node.style.transform = '';
			}
		});

		// Calculate the difference between old and new positions
		const deltaX = from.left - to.left;
		const deltaY = from.top - to.top;

		tl.fromTo(
			node,
			{
				x: deltaX,
				y: deltaY
			},
			{
				x: 0,
				y: 0,
				ease: 'power2.out'
			}
		);

		return {
			duration: (params.duration || 0.3) * 1000,
			delay: (params.delay || 0) * 1000,
			tick: (t) => {
				tl.progress(t);
			}
		};
	}

	let shouldShowTaskbar = $state(false);
	let isHovering = $state(false);
	let now = $state(new Date());

	onMount(() => {
		let screens = api.ipc.getScreens();
		return api.events.on('KeyListenerMessage', (e) => {
			if (e.mode !== 'mouse' || e.data.event_type !== 'move') return;
			const { x, y } = e.data;
			screens.forEach((screen) => {
				if (y >= screen.bounds.height - (shouldShowTaskbar ? 2 : 100)) {
					shouldShowTaskbar = true;
				} else {
					if (!(y >= screen.bounds.height - 100)) {
						openContextMenus = {};
					}
					if (!isHovering) {
						shouldShowTaskbar = false;
					}
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

	let openContextMenus: Record<string, boolean> = $state({});
</script>

<div class="relative flex h-[100vh] w-full items-end justify-center p-4 contain-content">
	<MouseEventsCapturer
		overlayId="taskbar"
		class="bg-background absolute right-2 top-2 flex transform flex-col items-end justify-center gap-1 rounded-lg border px-4 py-2 shadow transition-all duration-300"
		onMouseEvent={(e) => {
			e.preventDefault();
			if (e.type === 'enter') {
				e.target.classList.add('opacity-25');
			} else if (e.type === 'leave') {
				e.target.classList.remove('opacity-25');
			}
		}}
	>
		<div class="flex items-center gap-2 truncate text-xs">
			<img
				src={`data:image/png;base64,${api.native.taskbarItemList.icons[api.native.taskbarItemList.focusedItem?.executable_path || '']}`}
				class="h-3 w-3"
				alt="Focused Window Icon"
				draggable="false"
			/>
			<span class="max-w-64 truncate font-light opacity-95">
				{api.native.taskbarItemList.focusedItem?.title || 'Desktop'}
			</span>
		</div>
		<div class="flex text-sm font-semibold text-white">
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
		</div>
	</MouseEventsCapturer>

	<MouseEventsCapturer
		overlayId="taskbar"
		class="{shouldShowTaskbar
			? 'translate-y-0'
			: 'translate-y-32'} w-full drop-shadow-[0_8px_12px_rgba(0,0,0,0.5)] transition-all duration-300"
		onMouseEvent={(e) => {
			if (e.type === 'enter') {
				isHovering = true;
			} else if (e.type === 'leave') {
				isHovering = false;
			}
		}}
	>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<ContextMenu.Root
			onOpenChange={(open) => {
				openContextMenus = { taskbar: open };
			}}
			open={openContextMenus['taskbar']}
		>
			<ContextMenu.Trigger>
				<div class="bg-background flex items-center justify-between gap-4 rounded-lg border p-2">
					<div class="flex items-center gap-4 p-2">
						<ContextMenu.Root
							onOpenChange={(open) => {
								if (open) {
									openContextMenus = { startMenu: true };
								} else {
									delete openContextMenus.startMenu;
								}
							}}
							open={openContextMenus.startMenu}
						>
							<ContextMenu.Trigger>
								<button
									class="hover:bg-muted text-muted-foreground flex h-10 w-10 min-w-10 cursor-pointer items-center justify-center rounded-full border transition-all duration-300 hover:text-white"
									onclick={() => {
										api.native.taskbarItemList.openStartMenu();
									}}
								>
									<TrainTrack size={24} />
								</button>
							</ContextMenu.Trigger>
							<ContextMenu.Content>
								<ContextMenu.Item
									class="flex items-center gap-2"
									onclick={() => {
										api.native.taskbarItemList.startExecutable('cmd.exe');
									}}
								>
									<Terminal size={16} />
									<div class="max-w-64 truncate">Terminal</div>
								</ContextMenu.Item>
								<ContextMenu.Item
									class="flex items-center gap-2"
									onclick={() => {
										api.native.taskbarItemList.startExecutable('taskmgr.exe');
									}}
								>
									<LayoutList size={16} />
									Task Manager
								</ContextMenu.Item>
							</ContextMenu.Content>
						</ContextMenu.Root>
						<div class="bg-border h-12 w-0.5 min-w-0.5 rounded-full"></div>
						{#each api.native.taskbarItemList.taskbarItemsGrouped as group, index (group[0].hwnd)}
							{@const icon = api.native.taskbarItemList.icons[group[0].executable_path]}
							{@const isFocused = group.some((item) => item.is_focused)}
							{@const isRunning = group.some((item) => item.is_running)}
							<div class="flex" animate:smoothMove>
								<Tooltip.Provider delayDuration={100} disableCloseOnTriggerClick={true}>
									<Tooltip.Root>
										<Tooltip.Trigger>
											<ContextMenu.Root
												onOpenChange={(open) => {
													let ctxMenuId = `hwnd-${group[0].hwnd}`;
													if (open) {
														openContextMenus = { [ctxMenuId]: true };
													} else {
														delete openContextMenus[ctxMenuId];
													}
												}}
												open={openContextMenus[`hwnd-${group[0].hwnd}`]}
											>
												<ContextMenu.Trigger class="flex">
													<button
														class="{isFocused
															? 'scale-105 opacity-100'
															: 'opacity-50'} transition-all duration-300"
														onclick={() => {
															if (isRunning) {
																let hwnd = group.find((i) => i.hwnd)?.hwnd;
																if (!hwnd) return;
																api.native.taskbarItemList.toggleFocusWindow(hwnd);
																group.find((i) => i.hwnd)!.is_focused = true;
															} else {
																api.native.taskbarItemList.startExecutable(
																	group[0].executable_path
																);
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
															draggable="false"
														/>
													</button>
												</ContextMenu.Trigger>
												<ContextMenu.Content>
													<ContextMenu.Item
														class="flex items-center gap-2"
														onclick={() => {
															group.forEach((item) => {
																api.native.taskbarItemList.closeWindow(item.hwnd);
															});
														}}
													>
														<X size={16} />
														<div class="max-w-64 truncate">Close all windows</div>
													</ContextMenu.Item>
												</ContextMenu.Content>
											</ContextMenu.Root>
										</Tooltip.Trigger>
										<Tooltip.Content
											arrowClasses="hidden"
											class="bg-accent hide-when-taskbar-hidden p-0 text-white"
											sideOffset={32}
											side="top"
										>
											<MouseEventsCapturer
												class="flex gap-4 p-4"
												overlayId="taskbar"
												onMouseEvent={(e) => {
													if (e.type === 'enter') {
														isHovering = true;
													} else if (e.type === 'leave') {
														isHovering = false;
														shouldShowTaskbar = false;
													}
												}}
											>
												{#each group as item}
													{@const screenshot = api.native.taskbarItemList.screenshots[item.hwnd]}
													{@const icon = api.native.taskbarItemList.icons[item.executable_path]}
													<!-- svelte-ignore a11y_click_events_have_key_events -->
													<div
														class="rounded-lg opacity-90 transition-all duration-300 hover:opacity-100"
													>
														<div class="mb-2 flex items-center justify-between gap-2">
															<div class="flex items-center gap-2">
																<img
																	src={`data:image/png;base64,${icon}`}
																	class="h-6 w-6"
																	alt={item.title}
																/>
																<span class="max-w-48 truncate text-sm font-semibold"
																	>{item.title}</span
																>
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
														<div
															class="w-[256px] cursor-pointer"
															onclick={() => {
																api.native.taskbarItemList.toggleFocusWindow(item.hwnd);
																shouldShowTaskbar = false;
															}}
														>
															{#if screenshot?.data}
																<img
																	src={`data:image/png;base64,${screenshot.data}`}
																	class="rounded"
																	alt={group[0].title}
																	draggable="false"
																/>
															{:else}
																<div class="flex h-[128px] w-[256px] items-center justify-center">
																	<LoaderCircle
																		class="text-muted-foreground animate-spin"
																		size={24}
																		aria-label="Loading screenshot"
																	/>
																</div>
															{/if}
														</div>
													</div>
												{/each}
											</MouseEventsCapturer>
										</Tooltip.Content>
									</Tooltip.Root>
								</Tooltip.Provider>
							</div>
						{/each}
					</div>
					<div class="flex items-center gap-4">
						<div class="bg-border h-12 w-0.5 min-w-0.5 rounded-full"></div>
						<div
							class="flex h-14 w-64 select-none rounded-lg drop-shadow-[0_0_4px_rgba(0,0,0,0.5)] transition-all duration-300 contain-content"
							style="background-image: url('data:image/png;base64,{api.native.mediaInfo
								.artwork}'); background-size: cover; background-position: center;"
						>
							<div
								class="backdrop-blur-xs flex h-14 w-64 items-center justify-between p-1 backdrop-brightness-50"
							>
								<div
									class="flex w-full items-center justify-between gap-1 drop-shadow-[0_0_4px_rgba(0,0,0,0.5)]"
								>
									<div class="flex w-full items-center gap-2">
										<div
											class="h-12 w-12 min-w-12 rounded-lg transition-all duration-300"
											style="background-image: url('data:image/png;base64,{api.native.mediaInfo
												.artwork}'); background-size: cover; background-position: center;"
										></div>
										<div class="flex flex-col justify-center gap-1">
											<div class="flex w-full flex-col gap-1">
												<span class="max-w-30 truncate text-sm font-semibold leading-none">
													{api.native.mediaInfo.media?.title || 'No Media Playing'}
												</span>
												<span class="text-muted-foreground max-w-30 truncate text-xs">
													{api.native.mediaInfo.media?.artist || 'Unknown Artist'}
												</span>
											</div>
										</div>
									</div>
									<div class="flex h-12 w-16 min-w-16 items-center justify-between">
										<button
											onclick={() => {
												api.native.mediaInfo.previousTrack();
											}}
											class="text-muted-foreground cursor-pointer transition-all duration-300 hover:text-white"
										>
											<SkipBack size={20} />
										</button>
										<button
											onclick={() => {
												api.native.mediaInfo.togglePlayPause();
											}}
											class="text-muted-foreground cursor-pointer transition-all duration-300 hover:text-white"
										>
											{#if api.native.mediaInfo.media?.playback_status === 'Playing'}
												<Pause size={20} />
											{:else}
												<Play size={20} />
											{/if}
										</button>
										<button
											onclick={() => {
												api.native.mediaInfo.skipTrack();
											}}
											class="text-muted-foreground cursor-pointer transition-all duration-300 hover:text-white"
										>
											<SkipForward size={20} />
										</button>
									</div>
								</div>
							</div>
						</div>
					</div>
				</div>
			</ContextMenu.Trigger>
			<ContextMenu.Content>
				<ContextMenu.Item
					class="flex items-center gap-2"
					onclick={() => {
						api.native.taskbarItemList.startExecutable('taskmgr.exe');
					}}
				>
					<LayoutList size={16} />
					Task Manager
				</ContextMenu.Item>
				<ContextMenu.Separator />
				<ContextMenu.Item class="flex items-center gap-2">
					<Settings size={16} />
					UI Settings
				</ContextMenu.Item>
			</ContextMenu.Content>
		</ContextMenu.Root>
	</MouseEventsCapturer>
</div>
