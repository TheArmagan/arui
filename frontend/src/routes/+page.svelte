<script lang="ts">
	import { api } from '@/base/api';
	import { Button } from '$lib/components/ui/button/index.js';
</script>

<div class="flex w-full flex-col items-center justify-center gap-2">
	<h1>Welcome To ARUI - pre-alpha</h1>
	<div class="flex items-center gap-2">
		<Button
			onclick={async () => {
				let screens = api.ipc.getScreens();
				await api.ipc.createOverlayWindow({
					id: 'taskbar',
					screenId: screens[0].id,
					path: '/overlays/taskbar'
				});
				api.ipc.setOverlayWindowIgnoreMouseEvents('taskbar', true);
			}}
			variant="outline"
		>
			Create Taskbar Overlay
		</Button>
		<Button
			onclick={async () => {
				api.ipc.quit();
			}}
			variant="outline"
		>
			Quit
		</Button>
	</div>
</div>
