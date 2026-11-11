<script lang="ts" generics="T extends ContextMenuTarget">
	// Wraps a trigger element and shows a context menu built from core items plus every item
	// contributed through ctx.contextMenus.extend(target, ...). In small windows (the taskbar)
	// the menu opens in the shared 'menu' popup surface so the window bounds do not clip it.
	import type { Snippet } from 'svelte';
	import type { ContextMenuTarget, ContextMenuTargets, MenuItem } from '../api';
	import * as ContextMenu from './context-menu/index.js';
	import MenuItems from './MenuItems.svelte';
	import { getUiHost } from './host';

	let {
		target,
		data,
		items = [],
		class: className = '',
		disabled = false,
		children
	}: {
		target: T;
		data: ContextMenuTargets[T];
		/** Core items, or a function building them when the menu opens. */
		items?: MenuItem[] | (() => MenuItem[]);
		class?: string;
		disabled?: boolean;
		children: Snippet;
	} = $props();

	const host = getUiHost();
	const popup = host.preferPopupMenus();
	let merged = $state<MenuItem[]>([]);

	function build(): MenuItem[] {
		const core = typeof items === 'function' ? items() : items;
		const extra = host.menuItems(target, data);
		if (core.length && extra.length) {
			return [...core, ...extra.map((it, i) => (i === 0 ? { ...it, separatorBefore: true } : it))];
		}
		return [...core, ...extra];
	}

	function onPopupMenu(e: MouseEvent) {
		if (disabled) return;
		e.preventDefault();
		e.stopPropagation();
		const list = build();
		if (!list.length) return;
		host.popupMenu(list, {
			clientX: e.clientX,
			clientY: e.clientY,
			screenX: e.screenX,
			screenY: e.screenY
		});
	}
</script>

{#if popup || disabled}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class={className} oncontextmenu={onPopupMenu}>
		{@render children()}
	</div>
{:else}
	<ContextMenu.Root
		onOpenChange={(open) => {
			if (open) merged = build();
		}}
	>
		<ContextMenu.Trigger class={className}>
			{@render children()}
		</ContextMenu.Trigger>
		<ContextMenu.Content class="min-w-48">
			<MenuItems items={merged} />
		</ContextMenu.Content>
	</ContextMenu.Root>
{/if}
