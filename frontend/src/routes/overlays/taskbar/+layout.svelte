<script lang="ts">
	import { api } from '@/base/api';
	import { onMount } from 'svelte';

	onMount(() => {
		api.init('overlay');
		// @ts-ignore
		window.ARUIAPI = api;
		return () => {
			api.destroy();
		};
	});

	let { children } = $props();
</script>

{@render children()}
