// A theme is just tokens (mapped to --arui-<name>) plus optional extra CSS.
// Activate it in Settings > Appearance; the choice is stored in settings core.theme.
import { defineMod } from '@arui/sdk';

const tokens: Record<string, string> = {
	radius: '0.5rem',
	bg: 'oklch(0.975 0.008 85)',
	fg: 'oklch(0.24 0.02 60)',
	card: 'oklch(0.99 0.006 85)',
	'card-fg': 'oklch(0.24 0.02 60)',
	popover: 'oklch(0.985 0.007 85)',
	'popover-fg': 'oklch(0.24 0.02 60)',
	primary: 'oklch(0.3 0.03 60)',
	'primary-fg': 'oklch(0.97 0.01 85)',
	secondary: 'oklch(0.93 0.012 85)',
	'secondary-fg': 'oklch(0.28 0.02 60)',
	muted: 'oklch(0.925 0.013 85)',
	'muted-fg': 'oklch(0.5 0.02 60)',
	accent: 'oklch(0.91 0.02 75)',
	'accent-fg': 'oklch(0.24 0.02 60)',
	destructive: 'oklch(0.55 0.19 28)',
	border: 'oklch(0.3 0.03 60 / 14%)',
	input: 'oklch(0.3 0.03 60 / 18%)',
	ring: 'oklch(0.6 0.08 45)',
	highlight: 'oklch(0.62 0.14 42)',
	'bar-shadow': '0 6px 18px rgb(80 60 30 / 0.18)',
	'surface-shadow': '0 18px 40px -14px rgb(80 60 30 / 0.35)'
};

// A faint paper grain on the taskbar. Core surfaces expose stable arui-* class names for this.
const css = `
.arui-taskbar-bar {
	background-image: radial-gradient(oklch(0.3 0.03 60 / 0.05) 1px, transparent 1px);
	background-size: 4px 4px;
}
`;

export default defineMod({
	activate(ctx) {
		ctx.themes.register({ id: 'paper', name: 'Paper', tokens, css });
	}
});
