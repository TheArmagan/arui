<script lang="ts">
	import type { AruiContext } from '@arui/sdk';

	let { ctx }: { ctx: AruiContext } = $props();

	const greeting = $derived(ctx.settings.get<string>('greeting'));
	const title = $derived(ctx.state.focused?.title ?? 'Desktop');
</script>

<div class="widget" title={title}>
	<span class="greeting">{greeting}</span>
	<span class="count">{ctx.state.windows.length} windows</span>
</div>

<style>
	.widget {
		display: flex;
		flex-direction: column;
		justify-content: center;
		padding: 0 10px;
		height: 40px;
		border-radius: var(--arui-radius);
		border: 1px solid var(--arui-border);
		background: var(--arui-card);
		color: var(--arui-card-fg);
		font-size: 12px;
		line-height: 1.2;
	}
	.count {
		color: var(--arui-muted-fg);
		font-size: 11px;
	}
</style>
