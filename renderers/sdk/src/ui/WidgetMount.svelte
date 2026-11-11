<script lang="ts">
	// Renders one registered widget or surface component with its mod's context and an error
	// boundary, so a broken mod widget cannot take down the whole surface.
	import { setContext, type Component } from 'svelte';
	import type { AruiContext } from '../api';
	import { ARUI_CTX_KEY } from './host';

	let {
		component,
		ctx,
		props = {},
		label = ''
	}: {
		component: Component<any>;
		ctx: AruiContext;
		props?: Record<string, unknown>;
		label?: string;
	} = $props();

	// A new registration remounts this component, so the context is fixed for its lifetime.
	// svelte-ignore state_referenced_locally
	setContext(ARUI_CTX_KEY, ctx);
	const Comp = $derived(component);
</script>

<svelte:boundary
	onerror={(error) => {
		ctx.log.error(`widget ${label} crashed`, error);
	}}
>
	<Comp {...props} {ctx} />
	{#snippet failed(error, reset)}
		<button
			type="button"
			class="text-destructive inline-flex items-center rounded-md border border-current/30 px-2 text-xs"
			title={String((error as Error)?.message ?? error)}
			onclick={reset}
		>
			{label || ctx.mod.id} failed
		</button>
	{/snippet}
</svelte:boundary>
