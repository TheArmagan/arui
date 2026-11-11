import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { aruiShims } from './vite/arui-shims.ts';
import { aruiDevMods } from './vite/mods-dev.ts';

export default defineConfig({
	plugins: [tailwindcss(), svelte(), aruiShims(), aruiDevMods()],
	clearScreen: false,
	server: {
		port: 5169,
		strictPort: true,
		cors: true
	},
	preview: {
		port: 5169,
		strictPort: true
	},
	build: {
		outDir: 'dist',
		emptyOutDir: true,
		target: 'es2022',
		// Shim modules use top-level await; WebView2 is evergreen Chromium.
		modulePreload: { polyfill: false }
	},
	envPrefix: ['VITE_', 'TAURI_ENV_']
});
