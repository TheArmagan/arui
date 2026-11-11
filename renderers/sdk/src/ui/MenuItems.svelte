<script lang="ts">
	// Renders MenuItem[] inside a bits-ui context menu (recursive for submenus).
	import type { MenuItem } from '../api';
	import * as ContextMenu from './context-menu/index.js';
	import MenuIcon from './MenuIcon.svelte';
	import Self from './MenuItems.svelte';

	let { items }: { items: MenuItem[] } = $props();
</script>

{#each items as item, i (item.id)}
	{#if item.separatorBefore && i > 0}
		<ContextMenu.Separator />
	{/if}
	{#if item.children?.length}
		<ContextMenu.Sub>
			<ContextMenu.SubTrigger disabled={item.disabled}>
				<MenuIcon icon={item.icon} />
				<span class="max-w-64 truncate">{item.label}</span>
			</ContextMenu.SubTrigger>
			<ContextMenu.SubContent>
				<Self items={item.children} />
			</ContextMenu.SubContent>
		</ContextMenu.Sub>
	{:else}
		<ContextMenu.Item disabled={item.disabled} onSelect={() => item.onSelect()}>
			<MenuIcon icon={item.icon} />
			<span class="max-w-64 truncate">{item.label}</span>
		</ContextMenu.Item>
	{/if}
{/each}
