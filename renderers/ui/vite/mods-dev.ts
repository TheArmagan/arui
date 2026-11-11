// Dev/preview-only plugin that makes the example mods available to the browser mock backend.
//
//   GET /__arui/dev/mods.json   -> [{ manifest, baseUrl }] for every examples/mods/*/mod.json
//   GET /mods/<id>/<file>       -> files from that mod folder (dist/index.js, dist/style.css, ...)
//
// It also watches each mod's dist/ and mod.json and sends an `arui:mod-changed` HMR event, which
// the mock backend turns into the `arui://mod-changed` protocol event (hot reload).
// Extra mod folders can be added with ARUI_DEV_MODS_DIRS (path-delimiter separated parent dirs).

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Connect, Plugin, ViteDevServer } from 'vite';

const uiRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const examplesDir = path.resolve(uiRoot, '../../examples/mods');

const MIME: Record<string, string> = {
	'.js': 'text/javascript; charset=utf-8',
	'.mjs': 'text/javascript; charset=utf-8',
	'.css': 'text/css; charset=utf-8',
	'.json': 'application/json; charset=utf-8',
	'.svg': 'image/svg+xml',
	'.png': 'image/png',
	'.jpg': 'image/jpeg',
	'.webp': 'image/webp',
	'.woff2': 'font/woff2',
	'.map': 'application/json; charset=utf-8'
};

interface DevMod {
	id: string;
	dir: string;
	manifest: Record<string, unknown>;
}

function parentDirs(): string[] {
	const extra = (process.env.ARUI_DEV_MODS_DIRS ?? '').split(path.delimiter).filter(Boolean);
	return [examplesDir, ...extra];
}

function scanMods(): DevMod[] {
	const mods: DevMod[] = [];
	for (const parent of parentDirs()) {
		if (!fs.existsSync(parent)) continue;
		for (const name of fs.readdirSync(parent)) {
			const dir = path.join(parent, name);
			const manifestPath = path.join(dir, 'mod.json');
			if (!fs.existsSync(manifestPath)) continue;
			try {
				const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
				if (typeof manifest.id === 'string') mods.push({ id: manifest.id, dir, manifest });
			} catch {
				// Ignore broken manifests; the folder is skipped until it parses again.
			}
		}
	}
	return mods;
}

function middleware(): Connect.NextHandleFunction {
	return (req, res, next) => {
		const url = decodeURIComponent(req.url?.split('?')[0] ?? '');
		if (url === '/__arui/dev/mods.json') {
			const list = scanMods().map((m) => ({ manifest: m.manifest, baseUrl: `/mods/${m.id}/` }));
			res.setHeader('Content-Type', 'application/json; charset=utf-8');
			res.setHeader('Cache-Control', 'no-store');
			res.end(JSON.stringify(list));
			return;
		}
		const match = /^\/mods\/([^/]+)\/(.+)$/.exec(url);
		if (!match) return next();
		const mod = scanMods().find((m) => m.id === match[1]);
		if (!mod) return next();
		const file = path.resolve(mod.dir, match[2]);
		// Refuse paths that escape the mod folder.
		if (!file.startsWith(mod.dir + path.sep) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
			res.statusCode = 404;
			res.end('not found');
			return;
		}
		res.setHeader('Content-Type', MIME[path.extname(file)] ?? 'application/octet-stream');
		res.setHeader('Cache-Control', 'no-store');
		res.setHeader('Access-Control-Allow-Origin', '*');
		fs.createReadStream(file).pipe(res);
	};
}

function watchMods(server: ViteDevServer) {
	const timers = new Map<string, ReturnType<typeof setTimeout>>();
	const watched = new Set<string>();
	const refresh = () => {
		for (const mod of scanMods()) {
			for (const p of [path.join(mod.dir, 'dist'), path.join(mod.dir, 'mod.json')]) {
				if (!watched.has(p)) {
					watched.add(p);
					server.watcher.add(p);
				}
			}
		}
	};
	refresh();
	const onChange = (file: string) => {
		const abs = path.resolve(file);
		const mod = scanMods().find(
			(m) => abs === path.join(m.dir, 'mod.json') || abs.startsWith(path.join(m.dir, 'dist') + path.sep)
		);
		if (!mod) return;
		clearTimeout(timers.get(mod.id));
		timers.set(
			mod.id,
			setTimeout(() => {
				server.ws.send({ type: 'custom', event: 'arui:mod-changed', data: { id: mod.id } });
				server.config.logger.info(`[arui] mod changed: ${mod.id}`, { timestamp: true });
			}, 120)
		);
	};
	server.watcher.on('add', onChange);
	server.watcher.on('change', onChange);
	server.watcher.on('unlink', onChange);
	// Pick up dist folders created after the server started.
	setInterval(refresh, 3000).unref();
}

export function aruiDevMods(): Plugin {
	return {
		name: 'arui-dev-mods',
		configureServer(server) {
			server.middlewares.use(middleware());
			watchMods(server);
		},
		configurePreviewServer(server) {
			server.middlewares.use(middleware());
		}
	};
}
