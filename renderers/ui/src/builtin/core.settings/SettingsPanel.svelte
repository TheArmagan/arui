<script lang="ts">
	// 'settings' panel: mods (enable/disable), theme picker, and a form for every mod that called
	// ctx.settings.define(). Mods can add whole pages through the settings.pages slot.
	import type { AruiContext, InstalledMod } from '@arui/sdk';
	import { Button, Slot } from '@arui/sdk/ui';
	import FolderOpen from '@lucide/svelte/icons/folder-open';
	import Palette from '@lucide/svelte/icons/palette';
	import Puzzle from '@lucide/svelte/icons/puzzle';
	import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal';
	import X from '@lucide/svelte/icons/x';
	import Check from '@lucide/svelte/icons/check';
	import type { RuntimeInfo } from '../../lib/runtime/runtime.svelte';
	import Field from './Field.svelte';

	let { ctx }: { ctx: AruiContext } = $props();

	let info = $state.raw<RuntimeInfo | null>(null);
	// svelte-ignore state_referenced_locally
	ctx.commands.run('arui.runtime').then((v) => (info = v as RuntimeInfo));

	let page = $state<string>('mods');
	let busy = $state<string | null>(null);

	const mods = $derived(ctx.state.mods);
	const schemaMods = $derived(
		mods.filter((m) => m.enabled && info?.schemas[m.manifest.id]?.fields.length)
	);
	const themes = $derived(info?.themes ?? []);
	const activeTheme = $derived(ctx.settings.read<string>('core', 'theme') ?? 'default');
	const current = $derived(schemaMods.find((m) => m.manifest.id === page));

	async function toggle(m: InstalledMod) {
		if (!info) return;
		busy = m.manifest.id;
		try {
			await info.setModEnabled(m.manifest.id, !m.enabled);
		} finally {
			busy = null;
		}
	}

	function valueOf(modId: string, key: string, fallback: unknown) {
		return ctx.settings.read(modId, key) ?? fallback;
	}

	function setValue(modId: string, key: string, value: unknown) {
		void ctx.invoke('settings_set', { namespace: modId, key, value });
	}

	const swatch = (t: { tokens: Record<string, string> }, k: string, d: string) => t.tokens[k] ?? d;
</script>

<div class="h-full p-px">
	<div class="arui-surface bg-background flex h-full overflow-hidden">
		<nav class="bg-card/40 flex w-56 shrink-0 flex-col gap-0.5 border-r p-3">
			<div data-tauri-drag-region class="px-2 pt-1 pb-4">
				<div class="font-display text-lg font-semibold">Settings</div>
				<div class="text-muted-foreground text-xs">ARUI shell</div>
			</div>
			{#snippet navItem(id: string, label: string, Icon: typeof Puzzle)}
				<button
					type="button"
					class="flex items-center gap-2.5 rounded-md px-2.5 py-2 text-left text-sm transition-colors
						{page === id ? 'bg-accent text-accent-foreground font-medium' : 'text-muted-foreground hover:bg-muted/60 hover:text-foreground'}"
					onclick={() => (page = id)}
				>
					<Icon class="size-4" />
					<span class="truncate">{label}</span>
				</button>
			{/snippet}
			{@render navItem('mods', 'Mods', Puzzle)}
			{@render navItem('appearance', 'Appearance', Palette)}
			{#if schemaMods.length}
				<div class="text-muted-foreground mt-4 mb-1 px-2.5 text-[11px] font-medium tracking-wide uppercase">
					Mod settings
				</div>
				{#each schemaMods as m (m.manifest.id)}
					{@render navItem(m.manifest.id, info?.schemas[m.manifest.id]?.title ?? m.manifest.name, SlidersHorizontal)}
				{/each}
			{/if}
		</nav>

		<main class="flex min-w-0 flex-1 flex-col">
			<header data-tauri-drag-region class="flex items-center justify-between px-8 pt-6 pb-4">
				<h1 class="font-display text-2xl font-semibold">
					{page === 'mods' ? 'Mods' : page === 'appearance' ? 'Appearance' : (info?.schemas[page]?.title ?? current?.manifest.name)}
				</h1>
				<button
					type="button"
					aria-label="Close settings"
					class="text-muted-foreground hover:bg-muted hover:text-foreground rounded-md p-1.5"
					onclick={() => ctx.surface.close()}
				>
					<X size={18} />
				</button>
			</header>

			<div class="scrollbar-thin min-h-0 flex-1 overflow-y-auto px-8 pb-8">
				{#if page === 'mods'}
					<div class="mb-4 flex items-center justify-between">
						<p class="text-muted-foreground text-sm">
							Built-in parts of the shell are mods too. Changes apply to every window immediately.
						</p>
						<Button variant="outline" size="sm" onclick={() => ctx.invoke('mods_open_folder')}>
							<FolderOpen />
							Open mods folder
						</Button>
					</div>
					<div class="divide-y rounded-lg border">
						{#each mods as m (m.manifest.id)}
							{@const required = info?.isRequired(m.manifest.id)}
							{@const error = info?.errors[m.manifest.id]}
							<div class="flex items-center gap-4 px-4 py-3">
								<div class="min-w-0 flex-1">
									<div class="flex items-center gap-2">
										<span class="text-sm font-medium">{m.manifest.name}</span>
										<span class="text-muted-foreground text-xs">{m.manifest.version}</span>
										{#if m.builtin}
											<span class="bg-muted text-muted-foreground rounded px-1.5 py-0.5 text-[10px] font-medium">built-in</span>
										{/if}
									</div>
									<div class="text-muted-foreground truncate text-xs">
										{m.manifest.description ?? m.manifest.id}
									</div>
									{#if error}
										<div class="text-destructive mt-1 text-xs">{error}</div>
									{/if}
								</div>
								<button
									type="button"
									role="switch"
									aria-checked={m.enabled}
									aria-label="Enable {m.manifest.name}"
									disabled={required || busy === m.manifest.id}
									class="relative h-6 w-11 shrink-0 rounded-full border transition-colors disabled:opacity-40
										{m.enabled ? 'bg-highlight border-transparent' : 'bg-muted'}"
									onclick={() => toggle(m)}
								>
									<span
										class="absolute top-0.5 left-0.5 size-[18px] rounded-full shadow transition-transform
											{m.enabled ? 'translate-x-5 bg-black' : 'bg-foreground/80'}"
									></span>
								</button>
							</div>
						{/each}
					</div>
				{:else if page === 'appearance'}
					<p class="text-muted-foreground mb-4 text-sm">Themes come from mods. Pick one to apply it everywhere.</p>
					<div class="grid grid-cols-3 gap-3">
						{#each themes as t (t.id)}
							<button
								type="button"
								class="group overflow-hidden rounded-lg border text-left transition-colors
									{activeTheme === t.id ? 'border-highlight ring-highlight/40 ring-2' : 'hover:border-foreground/30'}"
								onclick={() => ctx.themes.activate(t.id)}
							>
								<div
									class="flex h-20 items-end gap-1.5 p-3"
									style:background={swatch(t, 'bg', 'var(--arui-bg)')}
								>
									<span class="h-6 flex-1 rounded" style:background={swatch(t, 'card', 'var(--arui-card)')}></span>
									<span class="size-6 rounded-full" style:background={swatch(t, 'primary', 'var(--arui-primary)')}></span>
									<span class="size-6 rounded-full" style:background={swatch(t, 'highlight', 'var(--arui-highlight)')}></span>
								</div>
								<div class="flex items-center justify-between px-3 py-2">
									<div class="min-w-0">
										<div class="truncate text-sm font-medium">{t.name}</div>
										<div class="text-muted-foreground truncate text-[11px]">{t.modId}</div>
									</div>
									{#if activeTheme === t.id}<Check class="text-highlight size-4" />{/if}
								</div>
							</button>
						{/each}
					</div>
				{:else if current && info}
					{@const schema = info.schemas[current.manifest.id]}
					<div class="divide-y">
						{#each schema.fields as field (field.key)}
							<Field
								{field}
								value={valueOf(current.manifest.id, field.key, field.default)}
								onchange={(v) => setValue(current.manifest.id, field.key, v)}
							/>
						{/each}
					</div>
				{/if}
				<Slot name="settings.pages" props={{ page }} />
			</div>
		</main>
	</div>
</div>
