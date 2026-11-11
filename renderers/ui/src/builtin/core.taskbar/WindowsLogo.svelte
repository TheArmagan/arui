<script lang="ts">
	// Monochrome Windows logo (four squares). Takes a lucide-style `size` so it also works as a
	// MenuItem icon; the color follows currentColor.
	let { size = 16, class: className = '' }: { size?: number; class?: string } = $props();
</script>

<svg
	width={size}
	height={size}
	viewBox="0 0 16 16"
	fill="currentColor"
	class={className}
	aria-hidden="true"
>
	<rect x="1" y="1" width="6.6" height="6.6" rx="0.4" />
	<rect x="8.4" y="1" width="6.6" height="6.6" rx="0.4" />
	<rect x="1" y="8.4" width="6.6" height="6.6" rx="0.4" />
	<rect x="8.4" y="8.4" width="6.6" height="6.6" rx="0.4" />
</svg>
