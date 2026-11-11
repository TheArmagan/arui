<script lang="ts">
	// The 'taskbar' surface: a floating rounded bar (ported from the legacy overlay) with five
	// widget slots. Everything inside is a widget, so mods can add, reorder or hide parts.
	import type { AruiContext, MenuItem } from '@arui/sdk';
	import { ContextMenuHost, Slot } from '@arui/sdk/ui';
	import LayoutList from '@lucide/svelte/icons/layout-list';
	import Power from '@lucide/svelte/icons/power';
	import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
	import Settings from '@lucide/svelte/icons/settings';

	let { ctx }: { ctx: AruiContext } = $props();

	const barItems = (): MenuItem[] => [
		{
			id: 'taskmgr',
			label: 'Task Manager',
			icon: LayoutList,
			onSelect: () => void ctx.actions.shellExecute('taskmgr.exe')
		},
		{
			id: 'settings',
			label: 'ARUI settings',
			icon: Settings,
			separatorBefore: true,
			onSelect: () => void ctx.surfaces.open('settings')
		},
		{
			id: 'restart',
			label: 'Restart ARUI',
			icon: RotateCcw,
			onSelect: () => void ctx.actions.restart()
		},
		{ id: 'quit', label: 'Quit ARUI', icon: Power, onSelect: () => void ctx.actions.quit() }
	];
	const slotProps = { orientation: 'horizontal' };
</script>

<div class="taskbar-window flex h-full w-full items-stretch px-2 pt-1.5 pb-2">
	<ContextMenuHost
		target="taskbar"
		data={{ monitor: ctx.surface.monitor }}
		items={barItems}
		class="flex min-w-0 flex-1"
	>
		<div
			class="arui-taskbar-bar bg-background/95 flex min-w-0 flex-1 items-center gap-3 rounded-[calc(var(--radius)+4px)] border px-2.5 shadow-(--arui-bar-shadow)"
		>
			<div class="flex shrink-0 items-center gap-3">
				<Slot name="taskbar.start" props={slotProps} />
			</div>
			<div class="bg-border h-8 w-0.5 shrink-0 rounded-full"></div>
			<div class="flex shrink-0 items-center gap-2 empty:hidden">
				<Slot name="taskbar.left" props={slotProps} />
			</div>
			<div class="flex min-w-0 flex-1 items-center gap-1 overflow-hidden">
				<Slot name="taskbar.center" props={slotProps} />
			</div>
			<div class="flex shrink-0 items-center gap-3 empty:hidden">
				<Slot name="taskbar.right" props={slotProps} />
			</div>
			<div class="bg-border h-8 w-0.5 shrink-0 rounded-full"></div>
			<div class="flex shrink-0 items-center gap-1">
				<Slot name="taskbar.end" props={slotProps} />
			</div>
		</div>
	</ContextMenuHost>
</div>
