<script lang="ts">
	// Pinned and running apps, one button per group. Running apps get a dot under the icon
	// that grows into an accent pill when focused. Hover asks core.previews for a preview popup.
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

	// Clicking or right clicking dismisses the preview so it never covers a menu.
	function dismissPreview(group: AppGroup) {
		ctx.commands.run('core.previews:leave', { appId: group.appId, immediate: true }).catch(() => {});
	}

	function onClick(group: AppGroup, e: MouseEvent) {
		dismissPreview(group);
		if (e.shiftKey) void launchNew(ctx, group);
		else void ctx.actions.activateGroup(group);
	}
</script>

<div class="flex items-center gap-0.5 {vertical ? 'flex-col' : ''}">
	{#each ctx.state.groups as group (group.appId)}
		{@const running = group.windows.length > 0}
		{@const lit = group.isFocused || group.isFlashing}
		<div class="flex" animate:flip={{ duration: 180 }} in:scale={{ duration: 140, start: 0.85 }}>
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
					class="arui-bar-button relative flex size-9 items-center justify-center
						{group.isFocused ? 'bg-white/[0.09] shadow-[inset_0_1px_0_rgb(255_255_255/0.08)]' : ''}"
					onclick={(e) => onClick(group, e)}
					onauxclick={(e) => {
						if (e.button === 1) void launchNew(ctx, group);
					}}
					onmousedown={() => dismissPreview(group)}
					oncontextmenucapture={() => dismissPreview(group)}
					onmouseenter={(e) => hover(group, e.currentTarget, true)}
					onmouseleave={(e) => hover(group, e.currentTarget, false)}
				>
					<img
						src={ctx.iconUrl(group.iconKey)}
						alt=""
						draggable="false"
						class="size-6 {group.isFlashing ? 'animate-pulse' : ''}"
					/>
					{#if running}
						<span
							class="absolute rounded-full transition-[width,height,background-color] duration-200 ease-out
								{vertical ? 'top-1/2 left-[2px] w-[3px] -translate-y-1/2' : 'bottom-[2px] left-1/2 h-[3px] -translate-x-1/2'}
								{group.isFlashing
								? 'bg-amber-400 shadow-[0_0_6px_var(--color-amber-400)]'
								: group.isFocused
									? 'bg-highlight shadow-[0_0_6px_var(--arui-highlight)]'
									: 'bg-muted-foreground/70'}
								{lit
								? vertical
									? 'h-3.5'
									: 'w-3.5'
								: group.windows.length > 1
									? vertical
										? 'h-2'
										: 'w-2'
									: vertical
										? 'h-1'
										: 'w-1'}"
						></span>
					{/if}
				</button>
			</ContextMenuHost>
		</div>
	{/each}
</div>
