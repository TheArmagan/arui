// Focus timer: a taskbar widget with a settings schema and a taskbar context menu extension.

import { defineMod } from '@arui/sdk';
import Timer from '@lucide/svelte/icons/timer';
import TimerReset from '@lucide/svelte/icons/timer-reset';
import FocusTimer from './FocusTimer.svelte';
import { read, reset, start } from './timer';

export default defineMod({
	activate(ctx) {
		ctx.settings.define({
			title: 'Focus timer',
			fields: [
				{
					key: 'minutes',
					label: 'Session length (minutes)',
					type: 'number',
					default: 25,
					min: 1,
					max: 120,
					step: 1
				},
				{ key: 'showSeconds', label: 'Show seconds', type: 'boolean', default: true }
			]
		});

		ctx.widgets.register('taskbar.right', FocusTimer, { id: 'focus-timer:widget', order: 400 });

		ctx.contextMenus.extend('taskbar', () => {
			const running = read(ctx).status !== 'idle';
			return [
				{
					id: 'focus-timer:start',
					label: running ? 'Restart focus session' : 'Start focus session',
					icon: Timer,
					onSelect: () => void start(ctx)
				},
				{
					id: 'focus-timer:reset',
					label: 'Reset focus timer',
					icon: TimerReset,
					disabled: !running,
					onSelect: () => void reset(ctx)
				}
			];
		});
	}
});
