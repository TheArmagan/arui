// core.theme-default: the dark theme ported from the legacy shadcn tokens (app.css `.dark`).

import { defineMod, type ModManifest } from '@arui/sdk';

export const manifest: ModManifest = {
	id: 'core.theme-default',
	name: 'Default theme',
	version: '0.1.0',
	apiVersion: 1,
	description: 'Neutral dark theme. Other themes only need to override the tokens they change.'
};

export const DEFAULT_TOKENS: Record<string, string> = {
	radius: '0.625rem',
	blur: '18px',
	bg: 'oklch(0.145 0 0)',
	fg: 'oklch(0.985 0 0)',
	card: 'oklch(0.205 0 0)',
	'card-fg': 'oklch(0.985 0 0)',
	popover: 'oklch(0.185 0 0)',
	'popover-fg': 'oklch(0.985 0 0)',
	primary: 'oklch(0.922 0 0)',
	'primary-fg': 'oklch(0.205 0 0)',
	secondary: 'oklch(0.269 0 0)',
	'secondary-fg': 'oklch(0.985 0 0)',
	muted: 'oklch(0.269 0 0)',
	'muted-fg': 'oklch(0.708 0 0)',
	accent: 'oklch(0.269 0 0)',
	'accent-fg': 'oklch(0.985 0 0)',
	destructive: 'oklch(0.704 0.191 22.216)',
	border: 'oklch(1 0 0 / 10%)',
	input: 'oklch(1 0 0 / 15%)',
	ring: 'oklch(0.556 0 0)',
	highlight: 'oklch(0.78 0.13 235)'
};

export default defineMod({
	activate(ctx) {
		ctx.themes.register({ id: 'default', name: 'ARUI Dark', tokens: DEFAULT_TOKENS });
	}
});
