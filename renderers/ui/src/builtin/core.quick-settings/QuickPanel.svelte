<script lang="ts">
	// 'quick-settings' popup: renders the quick.settings slot, which any mod can extend.
	import type { AruiContext } from '@arui/sdk';
	import { Slot } from '@arui/sdk/ui';
	import Settings from '@lucide/svelte/icons/settings';

	let { ctx }: { ctx: AruiContext } = $props();
	let height = $state(0);
	$effect(() => {
		if (height > 0) void ctx.surface.resize({ height: Math.ceil(height) + 2 });
	});
</script>

<div class="p-px">
	<div bind:clientHeight={height} class="arui-surface flex flex-col gap-3 p-3">
		<Slot name="quick.settings" />
		<div class="flex items-center justify-between border-t pt-3">
			<span class="text-muted-foreground text-xs">Quick settings</span>
			<button
				type="button"
				aria-label="Open ARUI settings"
				class="text-muted-foreground hover:bg-muted hover:text-foreground rounded-md p-1.5"
				onclick={() => {
					void ctx.surfaces.open('settings');
					void ctx.surface.close();
				}}
			>
				<Settings size={16} />
			</button>
		</div>
	</div>
</div>
