#!/usr/bin/env node
// Scaffold a new ARUI mod.
//
//   pnpm create arui-mod <dir>            (once published)
//   node renderers/create-arui-mod <dir>  (from the ARUI repo)
//
// Options: --id <mod-id>  --name "<Display name>"

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const templateDir = path.join(here, 'template');

function parseArgs(argv) {
	const out = { dir: null, id: null, name: null };
	for (let i = 0; i < argv.length; i++) {
		const a = argv[i];
		if (a === '--id') out.id = argv[++i];
		else if (a === '--name') out.name = argv[++i];
		else if (a === '-h' || a === '--help') out.help = true;
		else if (!out.dir) out.dir = a;
	}
	return out;
}

function slug(s) {
	return s
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-+|-+$/g, '');
}

function titleCase(s) {
	return s.replace(/[-_]+/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

/** Walk up from `start` looking for the ARUI repo (it contains renderers/sdk). */
function findSdk(start) {
	let dir = path.resolve(start);
	for (;;) {
		const sdk = path.join(dir, 'renderers', 'sdk', 'package.json');
		if (fs.existsSync(sdk)) return { repo: dir, sdk: path.dirname(sdk) };
		const parent = path.dirname(dir);
		if (parent === dir) return null;
		dir = parent;
	}
}

function svelteVersion(repo) {
	try {
		const ui = JSON.parse(fs.readFileSync(path.join(repo, 'renderers', 'ui', 'package.json'), 'utf8'));
		return ui.dependencies?.svelte ?? ui.devDependencies?.svelte;
	} catch {
		return null;
	}
}

function copyDir(src, dest, vars) {
	fs.mkdirSync(dest, { recursive: true });
	for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
		const from = path.join(src, entry.name);
		const toName = entry.name === '_gitignore' ? '.gitignore' : entry.name;
		const to = path.join(dest, toName);
		if (entry.isDirectory()) {
			copyDir(from, to, vars);
			continue;
		}
		const text = fs.readFileSync(from, 'utf8').replace(/\{\{(\w+)\}\}/g, (m, k) => vars[k] ?? m);
		fs.writeFileSync(to, text);
	}
}

const args = parseArgs(process.argv.slice(2));
if (args.help || !args.dir) {
	console.log('Usage: create-arui-mod <dir> [--id my-mod] [--name "My mod"]');
	process.exit(args.help ? 0 : 1);
}

const target = path.resolve(args.dir);
if (fs.existsSync(target) && fs.readdirSync(target).length) {
	console.error(`error: ${target} exists and is not empty`);
	process.exit(1);
}

const base = path.basename(target);
const id = args.id ?? slug(base);
const found = findSdk(target) ?? findSdk(here);
let sdk = '^0.1.0';
if (found) {
	const insideRepo = path.relative(found.repo, target).split(path.sep)[0] !== '..';
	// Inside the ARUI pnpm workspace use the workspace package; elsewhere point at the checkout.
	sdk = insideRepo ? 'workspace:*' : `file:${found.sdk.replace(/\\/g, '/')}`;
}

const vars = {
	name: slug(base) || id,
	id,
	title: args.name ?? titleCase(base),
	sdk,
	svelte: (found && svelteVersion(found.repo)) || '5.57.3'
};

copyDir(templateDir, target, vars);

const rel = path.relative(process.cwd(), target) || '.';
console.log(`Created ARUI mod "${vars.title}" (${id}) in ${rel}

Next:
  cd ${rel}
  pnpm install
  pnpm dev     # builds into %APPDATA%\\ARUI\\mods\\${id} and hot reloads in ARUI
`);
