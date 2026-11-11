// Builds the mod as one ES module (dist/index.js) plus dist/style.css.
//
// `svelte`, `svelte/*` and `@arui/sdk(/*)` stay external: inside ARUI they resolve through the
// shell's import map to the copies the shell already runs, so the mod shares its Svelte runtime.
//
//   vite build                  -> ./dist
//   vite build --watch          -> ./dist, rebuilt on change
//   vite build --watch --mode deploy
//                               -> $ARUI_MODS_DIR/<id>/ (default %APPDATA%/ARUI/mods/<id>/),
//                                  mod.json copied too; ARUI hot reloads the mod on every build

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { defineConfig, type Plugin } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

const manifest = JSON.parse(fs.readFileSync(new URL('./mod.json', import.meta.url), 'utf8'));

function modsRoot(): string {
	if (process.env.ARUI_MODS_DIR) return process.env.ARUI_MODS_DIR;
	const appData = process.env.APPDATA ?? path.join(os.homedir(), 'AppData', 'Roaming');
	return path.join(appData, 'ARUI', 'mods');
}

const external = (id: string) =>
	id === 'svelte' || id.startsWith('svelte/') || id === '@arui/sdk' || id.startsWith('@arui/sdk/');

/** In deploy mode, copy mod.json (and assets/ if present) next to dist/. */
function copyManifest(outDir: string): Plugin {
	return {
		name: 'arui-copy-manifest',
		writeBundle() {
			const root = path.dirname(outDir);
			fs.mkdirSync(root, { recursive: true });
			fs.copyFileSync('mod.json', path.join(root, 'mod.json'));
			if (fs.existsSync('assets')) fs.cpSync('assets', path.join(root, 'assets'), { recursive: true });
		}
	};
}

export default defineConfig(({ mode }) => {
	const deploy = mode === 'deploy';
	const outDir = deploy ? path.join(modsRoot(), manifest.id, 'dist') : path.resolve('dist');
	return {
		plugins: [svelte(), ...(deploy ? [copyManifest(outDir)] : [])],
		build: {
			outDir,
			emptyOutDir: true,
			target: 'es2022',
			minify: false,
			cssCodeSplit: false,
			lib: {
				entry: 'src/index.ts',
				formats: ['es'],
				fileName: () => 'index.js',
				cssFileName: 'style'
			},
			rollupOptions: { external }
		}
	};
});
