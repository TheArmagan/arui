<script lang="ts">
	import type { AruiContext } from '@arui/sdk';
	import { ContextMenuHost, getUiHost } from '@arui/sdk/ui';
	import LayoutList from '@lucide/svelte/icons/layout-list';
	import SquareTerminal from '@lucide/svelte/icons/square-terminal';
	import WindowsLogo from './WindowsLogo.svelte';

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
			icon: WindowsLogo,
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
		class="arui-bar-button flex items-center justify-center text-foreground/90 hover:text-foreground size-9 min-w-9"
		onclick={open}
	>
		<WindowsLogo size={16} />
	</button>
</ContextMenuHost>
