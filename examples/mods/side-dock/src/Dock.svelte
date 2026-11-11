<script lang="ts">
	// A vertical dock built from the same state and actions as the core taskbar. Core widgets
	// (start button, tray, clock, ...) are reused by rendering the core slots vertically.
	import type { AppGroup, AruiContext } from '@arui/sdk';
	import { ContextMenuHost, Slot, getUiHost } from '@arui/sdk/ui';
	import X from '@lucide/svelte/icons/x';
	import Pin from '@lucide/svelte/icons/pin';
	import PinOff from '@lucide/svelte/icons/pin-off';
	import { flip } from 'svelte/animate';

	let { ctx }: { ctx: AruiContext } = $props();
	const host = getUiHost();
	const vertical = { orientation: 'vertical' };

	function hover(group: AppGroup, el: HTMLElement, entering: boolean) {
		ctx.commands
			.run(entering ? 'core.previews:hover' : 'core.previews:leave', {
				appId: group.appId,
				rect: host.screenRect(el.getBoundingClientRect()),
				monitor: ctx.surface.monitor
			})
			.catch(() => {});
	}

	function menu(group: AppGroup) {
		const items = [];
		if (group.pinned) {
			items.push({ id: 'unpin', label: 'Unpin', icon: PinOff, onSelect: () => ctx.actions.unpin(group.appId) });
		} else if (group.app) {
			const app = group.app;
			items.push({
				id: 'pin',
				label: 'Pin to dock',
				icon: Pin,
				onSelect: () =>
					ctx.actions.pin({
						id: app.id,
						appId: group.appId,
						name: group.name,
						iconKey: group.iconKey,
						launch: { kind: 'app', id: app.id }
					})
			});
		}
		if (group.windows.length) {
			items.push({
				id: 'close',
				label: 'Close all',
				icon: X,
				onSelect: () => group.windows.forEach((w) => void ctx.actions.closeWindow(w.hwnd))
			});
		}
		return items;
	}

	const focusedName = $derived(ctx.state.groups.find((g) => g.isFocused)?.name ?? '');
</script>

<div class="dock-window">
	<div class="dock arui-taskbar-bar">
		<div class="section">
			<Slot name="taskbar.start" props={vertical} />
		</div>
		<div class="divider"></div>
		<div class="apps">
			{#each ctx.state.groups as group (group.appId)}
				<div animate:flip={{ duration: 250 }}>
					<ContextMenuHost target="taskbar.group" data={{ group }} items={() => menu(group)}>
						<button
							type="button"
							class="app"
							class:focused={group.isFocused}
							class:running={group.windows.length > 0}
							class:flashing={group.isFlashing}
							aria-label={group.name}
							onclick={() => ctx.actions.activateGroup(group)}
							onmouseenter={(e) => hover(group, e.currentTarget, true)}
							onmouseleave={(e) => hover(group, e.currentTarget, false)}
						>
							<img src={ctx.iconUrl(group.iconKey)} alt="" draggable="false" />
							<span class="dot"></span>
						</button>
					</ContextMenuHost>
				</div>
			{/each}
		</div>
		{#if ctx.settings.get('labels') && focusedName}
			<div class="label" title={focusedName}>{focusedName}</div>
		{/if}
		<div class="divider"></div>
		<div class="section">
			<Slot name="taskbar.right" props={vertical} />
			<Slot name="taskbar.end" props={vertical} />
		</div>
	</div>
</div>

<style>
	.dock-window {
		height: 100%;
		padding: 4px;
		box-sizing: border-box;
	}
	.dock {
		height: 100%;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
		padding: 6px 0;
		box-sizing: border-box;
		border-radius: calc(var(--arui-radius) + 2px);
		border: 1px solid var(--arui-border);
		background: color-mix(in oklab, var(--arui-bg) 94%, transparent);
		box-shadow:
			var(--arui-bar-shadow),
			inset 0 1px 0 rgb(255 255 255 / 0.06);
		color: var(--arui-fg);
	}
	.section {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
	}
	.divider {
		width: 20px;
		height: 1px;
		background: var(--arui-border);
		flex-shrink: 0;
	}
	.apps {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		scrollbar-width: none;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 2px;
	}
	.app {
		position: relative;
		width: 36px;
		height: 36px;
		display: flex;
		align-items: center;
		justify-content: center;
		border: 0;
		border-radius: calc(var(--arui-radius) - 2px);
		background: transparent;
		cursor: pointer;
		transition:
			background 150ms ease-out,
			box-shadow 150ms ease-out,
			transform 150ms ease-out;
	}
	.app:hover {
		background: color-mix(in oklab, var(--arui-fg) 7%, transparent);
		box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.05);
	}
	.app:active {
		transform: scale(0.92);
	}
	.app img {
		width: 24px;
		height: 24px;
	}
	.app.focused {
		background: color-mix(in oklab, var(--arui-fg) 9%, transparent);
	}
	.dot {
		position: absolute;
		left: -3px;
		top: 50%;
		width: 3px;
		height: 0;
		border-radius: 3px;
		transform: translateY(-50%);
		background: var(--arui-muted-fg);
		transition: height 0.25s;
	}
	.app.running .dot {
		height: 4px;
	}
	.app.focused .dot {
		height: 14px;
		box-shadow: 0 0 6px var(--arui-highlight);
		background: var(--arui-highlight);
	}
	.app.flashing .dot {
		height: 14px;
		background: #f5b83d;
	}
	.label {
		max-width: 40px;
		font-size: 10px;
		line-height: 1.2;
		text-align: center;
		color: var(--arui-muted-fg);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
