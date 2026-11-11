<script lang="ts">
	// One auto-rendered settings field.
	import type { SettingField } from '@arui/sdk';

	let {
		field,
		value,
		onchange
	}: { field: SettingField; value: unknown; onchange: (v: unknown) => void } = $props();
	const id = $derived(`f-${field.key}`);
</script>

<div class="flex items-center justify-between gap-6 py-3">
	<div class="min-w-0">
		<label for={id} class="text-sm font-medium">{field.label}</label>
		{#if field.description}
			<p class="text-muted-foreground mt-0.5 text-xs">{field.description}</p>
		{/if}
	</div>
	<div class="shrink-0">
		{#if field.type === 'boolean'}
			<button
				{id}
				type="button"
				role="switch"
				aria-checked={!!value}
				aria-label={field.label}
				class="relative h-6 w-11 rounded-full border transition-colors {value ? 'bg-highlight border-transparent' : 'bg-muted'}"
				onclick={() => onchange(!value)}
			>
				<span
					class="absolute top-0.5 left-0.5 size-[18px] rounded-full shadow transition-transform
						{value ? 'translate-x-5 bg-black' : 'bg-foreground/80'}"
				></span>
			</button>
		{:else if field.type === 'number'}
			<div class="flex items-center gap-3">
				{#if field.min != null && field.max != null}
					<input
						type="range"
						min={field.min}
						max={field.max}
						step={field.step ?? 1}
						value={Number(value)}
						aria-label={field.label}
						class="accent-highlight w-36"
						oninput={(e) => onchange(Number(e.currentTarget.value))}
					/>
				{/if}
				<input
					{id}
					type="number"
					min={field.min}
					max={field.max}
					step={field.step ?? 1}
					value={Number(value)}
					class="bg-background w-20 rounded-md border px-2 py-1 text-right text-sm tabular-nums"
					onchange={(e) => onchange(Number(e.currentTarget.value))}
				/>
			</div>
		{:else if field.type === 'select'}
			<select
				{id}
				value={String(value)}
				class="bg-background rounded-md border px-2 py-1.5 text-sm"
				onchange={(e) => onchange(e.currentTarget.value)}
			>
				{#each field.options ?? [] as opt (opt.value)}
					<option value={opt.value}>{opt.label}</option>
				{/each}
			</select>
		{:else if field.type === 'color'}
			<input
				{id}
				type="color"
				value={String(value ?? '#000000')}
				class="h-8 w-12 cursor-pointer rounded border bg-transparent"
				onchange={(e) => onchange(e.currentTarget.value)}
			/>
		{:else}
			<input
				{id}
				type="text"
				value={String(value ?? '')}
				class="bg-background w-56 rounded-md border px-2 py-1.5 text-sm"
				onchange={(e) => onchange(e.currentTarget.value)}
			/>
		{/if}
	</div>
</div>
