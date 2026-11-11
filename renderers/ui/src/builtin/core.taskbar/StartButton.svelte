<script lang="ts">
	import type { AruiContext } from '@arui/sdk';
	import { ContextMenuHost, getUiHost } from '@arui/sdk/ui';
	import LayoutList from '@lucide/svelte/icons/layout-list';
	import SquareTerminal from '@lucide/svelte/icons/square-terminal';
	import TrainTrack from '@lucide/svelte/icons/train-track';
	import AppWindow from '@lucide/svelte/icons/app-window';

	let { ctx }: { ctx: AruiContext } = $props();
	const host = getUiHost();
	let el = $state<HTMLButtonElement>();

	function open() {
		void ctx.surfaces.toggle({
			id: 'start-menu',
			monitor: ctx.surface.monitor ?? undefined,
			rect: el ? host.screenRect(el.getBoundingClientRect()) : undefined
		});
	}
</script>

<ContextMenuHost
	target="taskbar"
	data={{ monitor: ctx.surface.monitor }}
	items={[
		{
			id: 'terminal',
			label: 'Terminal',
			icon: SquareTerminal,
			onSelect: () => void ctx.actions.shellExecute('wt.exe')
		},
		{
			id: 'taskmgr',
			label: 'Task Manager',
			icon: LayoutList,
			onSelect: () => void ctx.actions.shellExecute('taskmgr.exe')
		},
		{
			id: 'windows-start',
			label: 'Windows Start menu',
			icon: AppWindow,
			separatorBefore: true,
			onSelect: () => void ctx.actions.openWindowsStart()
		}
	]}
	class="flex"
>
	<button
		bind:this={el}
		type="button"
		aria-label="Start"
		class="text-muted-foreground hover:bg-muted hover:text-foreground flex size-10 min-w-10 cursor-pointer items-center justify-center rounded-full border transition-all duration-300 active:scale-95"
		onclick={open}
	>
		<TrainTrack size={22} />
	</button>
</ContextMenuHost>
