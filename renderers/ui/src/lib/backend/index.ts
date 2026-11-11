// Picks the backend: Tauri when running inside the shell app, otherwise the browser mock.

import { createMockBackend } from './mock/client';
import { createTauriBackend } from './tauri';
import { labelFor, readLocation, type Backend } from './types';

export * from './types';

export function isTauri(): boolean {
	return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

let backend: Backend | null = null;

export function getBackend(): Backend {
	if (!backend) {
		const label = labelFor(readLocation());
		backend = isTauri() ? createTauriBackend(label) : createMockBackend(label);
	}
	return backend;
}
