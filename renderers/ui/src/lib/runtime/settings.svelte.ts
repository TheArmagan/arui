// Reactive settings.json mirror: namespaces -> keys -> values. Writes are applied locally
// right away and persisted through `settings_set`; other windows learn about them through
// `arui://settings-changed`.

import type { SettingsFile } from '@arui/sdk';
import type { Backend } from '../backend';

export class SettingsStore {
	values = $state<SettingsFile>({});
	private listeners = new Set<(ns: string, key: string, value: unknown) => void>();

	constructor(private backend: Backend) {}

	async init() {
		await this.backend.listen('arui://settings-changed', ({ namespace, key, value }) =>
			this.apply(namespace, key, value)
		);
		try {
			this.values = await this.backend.invoke('settings_get');
		} catch (err) {
			console.error('[arui] settings_get failed', err);
		}
	}

	private apply(ns: string, key: string, value: unknown) {
		if (!this.values[ns]) this.values[ns] = {};
		this.values[ns][key] = value;
		for (const fn of this.listeners) fn(ns, key, value);
	}

	get<T = unknown>(ns: string, key: string): T | undefined {
		return this.values[ns]?.[key] as T | undefined;
	}

	async set(ns: string, key: string, value: unknown) {
		const plain = value === undefined ? null : JSON.parse(JSON.stringify(value));
		this.apply(ns, key, plain);
		await this.backend.invoke('settings_set', { namespace: ns, key, value: plain });
	}

	onChange(fn: (ns: string, key: string, value: unknown) => void): () => void {
		this.listeners.add(fn);
		return () => this.listeners.delete(fn);
	}
}
