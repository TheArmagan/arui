import { defineMod } from '@arui/sdk';
import Widget from './Widget.svelte';

export default defineMod({
	activate(ctx) {
		ctx.settings.define({
			title: '{{title}}',
			fields: [{ key: 'greeting', label: 'Greeting', type: 'string', default: 'Hello' }]
		});

		// Every widget and surface component receives its mod's `ctx` as a prop.
		ctx.widgets.register('taskbar.right', Widget, { id: '{{id}}:widget', order: 400 });

		ctx.log.info('activated in', ctx.surface.id);
		return () => ctx.log.info('disposed');
	}
});
