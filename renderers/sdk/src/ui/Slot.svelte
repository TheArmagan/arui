<script lang="ts">
	// Renders every widget registered for a slot, in order. Mods can render their own slots too.
	import type { Snippet } from 'svelte';
	import { getUiHost } from './host';
	import WidgetMount from './WidgetMount.svelte';

	let {
		name,
		props = {},
		fallback
	}: {
		name: string;
		/** Extra props passed to every widget (the widget's own registered props win). */
		props?: Record<string, unknown>;
		/** Rendered when the slot is empty. */
		fallback?: Snippet;
	} = $props();

	const host = getUiHost();
	const items = $derived(host.slot(name));
</script>

{#each items as item (item.entry.modId + '/' + item.entry.id)}
	<WidgetMount
		component={item.entry.component}
		ctx={item.ctx}
		props={{ ...props, ...item.entry.props }}
		label={item.entry.id}
	/>
{:else}
	{@render fallback?.()}
{/each}
