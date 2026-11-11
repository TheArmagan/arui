<script lang="ts">
	import type { AruiContext } from '@arui/sdk';
	import { networkIcon, networkLabel } from './icons';

	let { ctx }: { ctx: AruiContext } = $props();
	const net = $derived(ctx.state.network);
	const Icon = $derived(networkIcon(net));
</script>

<div class="bg-muted/50 flex items-center gap-3 rounded-lg p-2.5">
	<div
		class="flex size-9 shrink-0 items-center justify-center rounded-full
			{net.connected ? 'bg-highlight text-black' : 'bg-muted text-muted-foreground'}"
	>
		<Icon size={17} />
	</div>
	<div class="min-w-0 flex-1">
		<div class="truncate text-sm font-medium">{networkLabel(net)}</div>
		<div class="text-muted-foreground text-xs">
			{net.connected ? (net.internet ? 'Connected, secured' : 'No internet access') : 'Offline'}
		</div>
	</div>
	<button
		type="button"
		class="text-muted-foreground hover:text-foreground text-xs underline-offset-2 hover:underline"
		onclick={() => ctx.actions.shellExecute('ms-settings:network')}
	>
		Settings
	</button>
</div>
