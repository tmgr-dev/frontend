import type { ToastKind } from './types';

const GLYPHS: Record<ToastKind, string> = {
	decision: '?',
	agent: '✓',
	idle: '⏱',
	limit: '%',
};

export const glyphOf = (kind: ToastKind): string => GLYPHS[kind] ?? '•';

export const isOk = (kind: ToastKind): boolean => kind === 'agent';

export const ageLabel = (nowMs: number, atMs: number): string => {
	const minutes = Math.floor(Math.max(0, nowMs - atMs) / 60_000);
	if (minutes < 1) return 'now';
	if (minutes < 60) return `${minutes}m`;
	return `${Math.floor(minutes / 60)}h`;
};

export const heightOf = (contentHeight: number): number =>
	Math.ceil(Math.max(0, contentHeight));
