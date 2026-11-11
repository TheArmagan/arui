// Timer state lives in the mod's settings namespace, so every taskbar (one per monitor) shows the
// same countdown and it survives a reload.

import type { AruiContext } from '@arui/sdk';

export type TimerState =
	| { status: 'idle' }
	| { status: 'running'; endsAt: number; totalMs: number }
	| { status: 'paused'; remainingMs: number; totalMs: number };

export function read(ctx: AruiContext): TimerState {
	return ctx.settings.get<TimerState>('state') ?? { status: 'idle' };
}

export function durationMs(ctx: AruiContext): number {
	return Math.max(1, ctx.settings.get<number>('minutes') ?? 25) * 60_000;
}

export function remaining(state: TimerState, now: number, fallbackMs: number): number {
	if (state.status === 'running') return Math.max(0, state.endsAt - now);
	if (state.status === 'paused') return state.remainingMs;
	return fallbackMs;
}

export function start(ctx: AruiContext) {
	const totalMs = durationMs(ctx);
	return ctx.settings.set('state', { status: 'running', endsAt: Date.now() + totalMs, totalMs });
}

export function toggle(ctx: AruiContext) {
	const s = read(ctx);
	if (s.status === 'idle') return start(ctx);
	if (s.status === 'running') {
		const left = s.endsAt - Date.now();
		if (left <= 0) return reset(ctx);
		return ctx.settings.set('state', { status: 'paused', remainingMs: left, totalMs: s.totalMs });
	}
	return ctx.settings.set('state', {
		status: 'running',
		endsAt: Date.now() + s.remainingMs,
		totalMs: s.totalMs
	});
}

export function reset(ctx: AruiContext) {
	return ctx.settings.set('state', { status: 'idle' });
}
