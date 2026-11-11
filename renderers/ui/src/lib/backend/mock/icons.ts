// Generated placeholder images for the mock backend: app icons (gradient squircle + lucide
// glyph), tray glyphs and album artwork, all as SVG data URLs keyed like the real icon store.

import {
	AppWindow,
	Bluetooth,
	Calculator,
	Camera,
	Compass,
	Cloud,
	Code,
	Cpu,
	Disc3,
	Droplet,
	Folder,
	Gamepad2,
	Globe,
	Headphones,
	Image,
	Mail,
	MessageCircle,
	Music,
	NotebookPen,
	Palette,
	Settings,
	Shield,
	ShieldCheck,
	SquareTerminal,
	Store,
	Video,
	Zap,
	type IconNode
} from 'lucide';

interface AppVisual {
	from: string;
	to: string;
	glyph: IconNode;
}

const APPS: Record<string, AppVisual> = {
	explorer: { from: '#f6c445', to: '#e08a12', glyph: Folder },
	firefox: { from: '#ff8a3d', to: '#c2185b', glyph: Globe },
	edge: { from: '#2fc5d6', to: '#1460c4', glyph: Compass },
	code: { from: '#3aa7f0', to: '#1659a8', glyph: Code },
	terminal: { from: '#3b3f4a', to: '#15171c', glyph: SquareTerminal },
	spotify: { from: '#2ee06e', to: '#118a3c', glyph: Music },
	discord: { from: '#7d86ff', to: '#4752c4', glyph: MessageCircle },
	steam: { from: '#2a4b6e', to: '#0d1a2b', glyph: Gamepad2 },
	settings: { from: '#8a93a6', to: '#4a5163', glyph: Settings },
	calculator: { from: '#5c6370', to: '#2b2f36', glyph: Calculator },
	photos: { from: '#5ab0ff', to: '#7a4dff', glyph: Image },
	mail: { from: '#47a3ff', to: '#1e5fd9', glyph: Mail },
	notepad: { from: '#7fd1c7', to: '#2c8f86', glyph: NotebookPen },
	paint: { from: '#ff7eb3', to: '#ff4d6d', glyph: Palette },
	store: { from: '#4f8dff', to: '#3046c9', glyph: Store },
	camera: { from: '#6b7280', to: '#1f2937', glyph: Camera },
	vlc: { from: '#ff9f1c', to: '#e85d04', glyph: Video },
	taskmgr: { from: '#4ade80', to: '#0f766e', glyph: Cpu },
	obs: { from: '#4b5563', to: '#111827', glyph: Disc3 },
	defender: { from: '#38bdf8', to: '#0369a1', glyph: ShieldCheck },
	onedrive: { from: '#60a5fa', to: '#1d4ed8', glyph: Cloud },
	dropbox: { from: '#3b82f6', to: '#1e40af', glyph: Droplet },
	nvidia: { from: '#84cc16', to: '#3f6212', glyph: Zap },
	bluetooth: { from: '#60a5fa', to: '#2563eb', glyph: Bluetooth },
	headphones: { from: '#a78bfa', to: '#6d28d9', glyph: Headphones },
	security: { from: '#94a3b8', to: '#475569', glyph: Shield }
};

const cache = new Map<string, string>();

function hash(str: string): number {
	let h = 2166136261;
	for (let i = 0; i < str.length; i++) {
		h ^= str.charCodeAt(i);
		h = Math.imul(h, 16777619);
	}
	return h >>> 0;
}

function glyphMarkup(node: IconNode, stroke: string, strokeWidth = 2): string {
	return node
		.map(([tag, attrs]) => {
			const a = Object.entries(attrs)
				.map(([k, v]) => `${k}="${v}"`)
				.join(' ');
			return `<${tag} ${a} fill="none" stroke="${stroke}" stroke-width="${strokeWidth}" stroke-linecap="round" stroke-linejoin="round"/>`;
		})
		.join('');
}

function dataUrl(svg: string): string {
	return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

function appIcon(name: string): string {
	const visual = APPS[name] ?? {
		from: `hsl(${hash(name) % 360} 70% 60%)`,
		to: `hsl(${(hash(name) + 40) % 360} 70% 38%)`,
		glyph: AppWindow
	};
	const svg =
		`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">` +
		`<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">` +
		`<stop offset="0" stop-color="${visual.from}"/><stop offset="1" stop-color="${visual.to}"/>` +
		`</linearGradient></defs>` +
		`<rect x="4" y="4" width="56" height="56" rx="15" fill="url(#g)"/>` +
		`<rect x="4.5" y="4.5" width="55" height="55" rx="14.5" fill="none" stroke="#fff" stroke-opacity=".18"/>` +
		`<g transform="translate(16 16) scale(1.3333)">${glyphMarkup(visual.glyph, '#fff', 1.9)}</g>` +
		`</svg>`;
	return dataUrl(svg);
}

function trayIcon(name: string): string {
	const visual = APPS[name];
	const color = visual?.from ?? '#e5e5e5';
	const svg =
		`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">` +
		glyphMarkup(visual?.glyph ?? AppWindow, color, 2) +
		`</svg>`;
	return dataUrl(svg);
}

function artwork(seed: string): string {
	const h = hash(seed);
	const hue = h % 360;
	const hue2 = (hue + 50 + (h % 70)) % 360;
	const cx = 60 + (h % 140);
	const cy = 60 + ((h >> 8) % 140);
	const svg =
		`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">` +
		`<defs><linearGradient id="a" x1="0" y1="0" x2="1" y2="1">` +
		`<stop offset="0" stop-color="hsl(${hue} 75% 55%)"/><stop offset="1" stop-color="hsl(${hue2} 70% 22%)"/>` +
		`</linearGradient><radialGradient id="b"><stop offset="0" stop-color="#fff" stop-opacity=".55"/>` +
		`<stop offset="1" stop-color="#fff" stop-opacity="0"/></radialGradient></defs>` +
		`<rect width="256" height="256" fill="url(#a)"/>` +
		`<circle cx="${cx}" cy="${cy}" r="90" fill="url(#b)"/>` +
		`<circle cx="${256 - cx}" cy="${256 - cy}" r="46" fill="none" stroke="#fff" stroke-opacity=".35" stroke-width="10"/>` +
		`<path d="M0 ${190 + (h % 30)} Q128 ${140 + (h % 50)} 256 ${200 - (h % 40)} V256 H0Z" fill="#000" fill-opacity=".28"/>` +
		`</svg>`;
	return dataUrl(svg);
}

/** Map a mock icon-store key to an image URL. Keys: mock:app/<name>, mock:tray/<name>, mock:art/<seed>. */
export function mockIconUrl(key: string | null | undefined): string {
	if (!key) return '';
	const hit = cache.get(key);
	if (hit) return hit;
	let url: string;
	const [, kind, name] = /^mock:([a-z]+)\/(.+)$/.exec(key) ?? [];
	if (kind === 'tray') url = trayIcon(name);
	else if (kind === 'art') url = artwork(name);
	else url = appIcon(name ?? key);
	cache.set(key, url);
	return url;
}
