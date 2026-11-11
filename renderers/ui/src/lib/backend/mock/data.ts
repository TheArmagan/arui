// Seed data for the mock backend: a believable desktop session.

import type { AppEntry, MediaSession, MonitorInfo, TrayIcon, WindowInfo } from '@arui/sdk';

export function seedMonitors(width: number, height: number): MonitorInfo[] {
	return [
		{
			id: '\\\\.\\DISPLAY1',
			handle: 65537,
			bounds: { left: 0, top: 0, right: width, bottom: height },
			workArea: { left: 0, top: 0, right: width, bottom: height },
			scale: 1,
			primary: true
		},
		{
			id: '\\\\.\\DISPLAY2',
			handle: 65539,
			bounds: { left: width, top: 0, right: width + 2560, bottom: 1440 },
			workArea: { left: width, top: 0, right: width + 2560, bottom: 1440 },
			scale: 1.25,
			primary: false
		}
	];
}

interface AppSeed {
	name: string;
	key: string;
	exe: string | null;
	appId?: string;
	folder: string | null;
	packaged?: boolean;
}

const APP_SEEDS: AppSeed[] = [
	{ name: 'File Explorer', key: 'explorer', exe: 'C:\\Windows\\explorer.exe', folder: null },
	{ name: 'Firefox', key: 'firefox', exe: 'C:\\Program Files\\Mozilla Firefox\\firefox.exe', folder: null },
	{ name: 'Microsoft Edge', key: 'edge', exe: 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe', folder: null },
	{ name: 'Visual Studio Code', key: 'code', exe: 'C:\\Users\\user\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe', folder: 'Development' },
	{ name: 'Terminal', key: 'terminal', exe: null, appId: 'Microsoft.WindowsTerminal_8wekyb3d8bbwe!App', folder: null, packaged: true },
	{ name: 'Spotify', key: 'spotify', exe: 'C:\\Users\\user\\AppData\\Roaming\\Spotify\\Spotify.exe', folder: null },
	{ name: 'Discord', key: 'discord', exe: 'C:\\Users\\user\\AppData\\Local\\Discord\\app-1.0.9200\\Discord.exe', folder: 'Discord Inc' },
	{ name: 'Steam', key: 'steam', exe: 'C:\\Program Files (x86)\\Steam\\steam.exe', folder: 'Steam' },
	{ name: 'Settings', key: 'settings', exe: null, appId: 'windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel', folder: null, packaged: true },
	{ name: 'Calculator', key: 'calculator', exe: null, appId: 'Microsoft.WindowsCalculator_8wekyb3d8bbwe!App', folder: null, packaged: true },
	{ name: 'Photos', key: 'photos', exe: null, appId: 'Microsoft.Windows.Photos_8wekyb3d8bbwe!App', folder: null, packaged: true },
	{ name: 'Mail', key: 'mail', exe: null, appId: 'microsoft.windowscommunicationsapps_8wekyb3d8bbwe!microsoft.windowslive.mail', folder: null, packaged: true },
	{ name: 'Notepad', key: 'notepad', exe: 'C:\\Windows\\System32\\notepad.exe', folder: 'Accessories' },
	{ name: 'Paint', key: 'paint', exe: 'C:\\Windows\\System32\\mspaint.exe', folder: 'Accessories' },
	{ name: 'Microsoft Store', key: 'store', exe: null, appId: 'Microsoft.WindowsStore_8wekyb3d8bbwe!App', folder: null, packaged: true },
	{ name: 'Camera', key: 'camera', exe: null, appId: 'Microsoft.WindowsCamera_8wekyb3d8bbwe!App', folder: null, packaged: true },
	{ name: 'VLC media player', key: 'vlc', exe: 'C:\\Program Files\\VideoLAN\\VLC\\vlc.exe', folder: 'VideoLAN' },
	{ name: 'Task Manager', key: 'taskmgr', exe: 'C:\\Windows\\System32\\Taskmgr.exe', folder: 'Windows Tools' },
	{ name: 'OBS Studio', key: 'obs', exe: 'C:\\Program Files\\obs-studio\\bin\\64bit\\obs64.exe', folder: 'OBS Studio' },
	{ name: 'Windows Security', key: 'defender', exe: null, appId: 'Microsoft.SecHealthUI_8wekyb3d8bbwe!SecHealthUI', folder: 'Windows Tools', packaged: true },
	{ name: 'Command Prompt', key: 'terminal-cmd', exe: 'C:\\Windows\\System32\\cmd.exe', folder: 'Windows Tools' },
	{ name: 'Remote Desktop Connection', key: 'mstsc', exe: 'C:\\Windows\\System32\\mstsc.exe', folder: 'Accessories' },
	{ name: 'Snipping Tool', key: 'snip', exe: null, appId: 'Microsoft.ScreenSketch_8wekyb3d8bbwe!App', folder: null, packaged: true },
	{ name: 'Blender', key: 'blender', exe: 'C:\\Program Files\\Blender Foundation\\Blender 4.5\\blender.exe', folder: 'Blender' },
	{ name: 'GitHub Desktop', key: 'github-desktop', exe: 'C:\\Users\\user\\AppData\\Local\\GitHubDesktop\\GitHubDesktop.exe', folder: 'Development' },
	{ name: 'Node.js command prompt', key: 'node', exe: 'C:\\Program Files\\nodejs\\node.exe', folder: 'Development' }
];

export function seedApps(): AppEntry[] {
	return APP_SEEDS.map((s) => ({
		id: `app:${s.key}`,
		name: s.name,
		kind: s.packaged ? 'packaged' : 'shortcut',
		target: s.packaged ? `shell:AppsFolder\\${s.appId}` : (s.exe as string),
		exePath: s.exe,
		appId: s.appId ?? (s.exe ? s.exe.toLowerCase() : null),
		iconKey: `mock:app/${s.key.replace('terminal-cmd', 'terminal')}`,
		folder: s.folder
	}));
}

let nextHwnd = 0x2001a;
export function newHwnd(): number {
	nextHwnd += 0x1c4;
	return nextHwnd;
}

export function windowFor(app: AppEntry, title: string, monitor = 65537): WindowInfo {
	return {
		hwnd: newHwnd(),
		pid: 1000 + Math.floor(Math.random() * 30000),
		title,
		className: app.kind === 'packaged' ? 'ApplicationFrameWindow' : 'Chrome_WidgetWin_1',
		exePath: app.exePath ?? 'C:\\Windows\\System32\\ApplicationFrameHost.exe',
		appId: app.appId ?? app.id,
		iconKey: app.iconKey,
		monitor,
		isMinimized: false,
		isMaximized: false,
		isFocused: false,
		isFlashing: false
	};
}

export function seedWindows(apps: AppEntry[]): WindowInfo[] {
	const byKey = (key: string) => apps.find((a) => a.id === `app:${key}`)!;
	const list = [
		windowFor(byKey('firefox'), 'ARUI rewrite plan - GitHub - Mozilla Firefox'),
		windowFor(byKey('code'), 'api.ts - arui - Visual Studio Code'),
		windowFor(byKey('explorer'), 'Downloads'),
		windowFor(byKey('firefox'), 'Svelte 5 runes documentation - Mozilla Firefox'),
		windowFor(byKey('terminal'), 'pnpm --filter @arui/ui dev'),
		windowFor(byKey('spotify'), 'Spotify Premium'),
		windowFor(byKey('discord'), '#general | ARUI - Discord'),
		windowFor(byKey('explorer'), 'Documents'),
		windowFor(byKey('settings'), 'Settings')
	];
	list[1].isFocused = true;
	list[1].isMaximized = true;
	list[5].isMinimized = true;
	return list;
}

export function seedTray(): TrayIcon[] {
	const icon = (name: string, tooltip: string, exe: string | null, visible = true): TrayIcon => ({
		id: `tray:${name}`,
		ownerHwnd: newHwnd(),
		uid: 1,
		guid: null,
		callbackMessage: 0x8001,
		version: 4,
		tooltip,
		iconKey: `mock:tray/${name}`,
		visible,
		exePath: exe
	});
	return [
		icon('discord', 'Discord', 'C:\\Users\\user\\AppData\\Local\\Discord\\app-1.0.9200\\Discord.exe'),
		icon('steam', 'Steam', 'C:\\Program Files (x86)\\Steam\\steam.exe'),
		icon('onedrive', 'OneDrive - Up to date', 'C:\\Program Files\\Microsoft OneDrive\\OneDrive.exe'),
		icon('nvidia', 'NVIDIA Settings', null),
		icon('bluetooth', 'Bluetooth Devices', null),
		icon('defender', 'Windows Security - No actions needed', null),
		icon('dropbox', 'Dropbox 212.4.5767', 'C:\\Program Files (x86)\\Dropbox\\Client\\Dropbox.exe'),
		icon('headphones', 'Headset: 80% battery', null),
		icon('security', 'Hidden helper', null, false)
	];
}

export interface Track {
	title: string;
	artist: string;
	album: string;
	durationMs: number;
}

export const TRACKS: Track[] = [
	{ title: 'Night Drive', artist: 'Lumen Field', album: 'Low Light', durationMs: 214_000 },
	{ title: 'Paper Planes Over Kadikoy', artist: 'Deniz Kaya', album: 'Ferry Lines', durationMs: 187_000 },
	{ title: 'Static Bloom', artist: 'The Overcasts', album: 'Weather Report', durationMs: 243_000 },
	{ title: 'Ember', artist: 'North Wind Ensemble', album: 'Ember', durationMs: 198_000 }
];

export function sessionFor(index: number, status: MediaSession['status'], positionMs: number): MediaSession {
	const t = TRACKS[index % TRACKS.length];
	return {
		id: 'Spotify.exe',
		appId: 'C:\\Users\\user\\AppData\\Roaming\\Spotify\\Spotify.exe'.toLowerCase(),
		title: t.title,
		artist: t.artist,
		album: t.album,
		status,
		canPlay: true,
		canPause: true,
		canNext: true,
		canPrevious: true,
		positionMs,
		durationMs: t.durationMs,
		positionAtMs: Date.now(),
		artworkKey: `mock:art/${t.album}`
	};
}
