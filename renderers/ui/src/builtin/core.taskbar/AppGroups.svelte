<script lang="ts">
	// Pinned and running apps, one button per group. Focused groups are full size and opaque,
	// the rest are dimmed like the legacy bar. Hover asks core.previews for a preview popup.
	import type { AppGroup, AruiContext } from '@arui/sdk';
	import { ContextMenuHost, getUiHost } from '@arui/sdk/ui';
	import { flip } from 'svelte/animate';
	import { scale } from 'svelte/transition';
	import { groupMenu, launchNew } from './group-menu';

	let { ctx, orientation = 'horizontal' }: { ctx: AruiContext; orientation?: string } = $props();
	const host = getUiHost();
	const vertical = $derived(orientation === 'vertical');

	function hover(group: AppGroup, el: HTMLElement, entering: boolean) {
		const cmd = entering ? 'core.previews:hover' : 'core.previews:leave';
		ctx.commands
			.run(cmd, {
				appId: group.appId,
				rect: host.screenRect(el.getBoundingClientRect()),
				monitor: ctx.surface.monitor
			})
			.catch(() => {
				// Previews disabled: nothing to do.
			});
	}

	function onClick(group: AppGroup, e: MouseEvent) {
		ctx.commands.run('core.previews:leave', { appId: group.appId, immediate: true }).catch(() => {});
		if (e.shiftKey) void launchNew(ctx, group);
		else void ctx.actions.activateGroup(group);
	}
</script>

<div class="flex items-center gap-1 {vertical ? 'flex-col' : ''}">
	{#each ctx.state.groups as group (group.appId)}
		{@const running = group.windows.length > 0}
		<div class="flex" animate:flip={{ duration: 260 }} in:scale={{ duration: 180, start: 0.6 }}>
			<ContextMenuHost
				target="taskbar.group"
				data={{ group }}
				items={() => groupMenu(ctx, group)}
				class="flex"
			>
				<button
					type="button"
					aria-label={group.name}
					data-focused={group.isFocused}
					class="group/app relative flex size-11 cursor-pointer items-center justify-center rounded-lg transition-colors duration-200 hover:bg-white/[0.06]
						{group.isFocused ? 'bg-white/[0.07]' : ''}"
					onclick={(e) => onClick(group, e)}
					onauxclick={(e) => {
						if (e.button === 1) void launchNew(ctx, group);
					}}
					onmouseenter={(e) => hover(group, e.currentTarget, true)}
					onmouseleave={(e) => hover(group, e.currentTarget, false)}
				>
					<img
						src={ctx.iconUrl(group.iconKey)}
						alt=""
						draggable="false"
						class="size-8 transition-all duration-300 group-hover/app:scale-110
							{group.isFocused ? 'scale-105 opacity-100' : running ? 'opacity-75' : 'opacity-50'}
							{group.isFlashing ? 'animate-pulse' : ''}"
					/>
					{#if running}
						<span
							class="absolute rounded-full transition-all duration-300
								{vertical ? 'top-1/2 left-0.5 w-[3px] -translate-y-1/2' : 'bottom-0.5 left-1/2 h-[3px] -translate-x-1/2'}
								{group.isFlashing
								? 'bg-amber-400 ' + (vertical ? 'h-4' : 'w-4')
								: group.isFocused
									? 'bg-highlight ' + (vertical ? 'h-4' : 'w-4')
									: 'bg-muted-foreground ' + (vertical ? 'h-1.5' : 'w-1.5')}"
						></span>
						{#if group.windows.length > 1 && !vertical}
							<span
								class="bg-muted-foreground/60 absolute bottom-0.5 left-1/2 h-[3px] w-1 translate-x-[9px] rounded-full"
							></span>
						{/if}
					{/if}
				</button>
			</ContextMenuHost>
		</div>
	{/each}
</div>
