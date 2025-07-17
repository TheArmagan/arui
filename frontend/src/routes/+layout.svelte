<script lang="ts">
	import { onMount } from 'svelte';
	import '../app.css';
	import { api } from '$lib/base/api';

	let { children } = $props();

	onMount(() => {
		api.init();
		// @ts-ignore
		window.ARUIAPI = api;
		return () => {
			api.destroy();
		};
	});
</script>

{@render children()}
