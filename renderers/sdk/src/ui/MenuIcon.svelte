<script lang="ts">
	// Icon for a MenuItem: a component (e.g. a lucide icon) or an image URL.
	import type { Component } from 'svelte';

	let { icon, class: className = 'size-4' }: { icon?: Component<any> | string; class?: string } =
		$props();
	const Icon = $derived(typeof icon === 'function' ? (icon as Component<any>) : null);
</script>

{#if typeof icon === 'string' && icon}
	<img src={icon} alt="" class="{className} shrink-0 object-contain" draggable="false" />
{:else if Icon}
	<Icon class={className} />
{:else}
	<span class="{className} shrink-0"></span>
{/if}
