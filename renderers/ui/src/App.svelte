<script lang="ts">
	// Root of a surface window: provides the UI host context and renders the component that
	// currently provides this window's surface id (a later registration replaces it live).
	import { setContext } from 'svelte';
	import { ARUI_HOST_KEY, WidgetMount } from '@arui/sdk/ui';
	import type { Runtime } from './lib/runtime/runtime.svelte';

	let { runtime }: { runtime: Runtime } = $props();

	// svelte-ignore state_referenced_locally
	setContext(ARUI_HOST_KEY, runtime);
	// svelte-ignore state_referenced_locally
	const id = runtime.location.surface;
	const reg = $derived(runtime.surfaceDef(id));

	$effect(() => {
		document.documentElement.dataset.surface = id;
		document.documentElement.dataset.kind = reg?.def.window.kind ?? 'none';
	});
</script>

{#if id === 'host'}
	<div class="text-muted-foreground p-4 font-mono text-xs">
		ARUI host window: {runtime.surfaceSpecs().length} surfaces defined.
	</div>
{:else if reg}
	{#key reg}
		<WidgetMount component={reg.def.component} ctx={reg.ctx} label="surface:{id}" />
	{/key}
{:else if runtime.ready}
	<div class="text-muted-foreground flex h-full items-center justify-center p-4 text-xs">
		No mod provides the surface "{id}".
	</div>
{/if}
