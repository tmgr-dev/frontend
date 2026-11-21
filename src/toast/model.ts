import type { ToastKind, ToastView } from './types';

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

export const progressFraction = (
	remainingMs?: number | null,
	totalMs?: number | null,
): number | null => {
	if (remainingMs == null || !totalMs || totalMs <= 0) return null;
	return Math.min(1, Math.max(0, remainingMs / totalMs));
};

export const drainStyle = (
	item: Pick<ToastView, 'remainingMs' | 'totalMs' | 'paused'>,
): Record<string, string> | null => {
	const fraction = progressFraction(item.remainingMs, item.totalMs);
	if (fraction === null) return null;
	return {
		'--from': String(fraction),
		'--drain': `${Math.max(0, item.remainingMs ?? 0)}ms`,
		'--state': item.paused ? 'paused' : 'running',
	};
};

export const hoverKey = (hover: boolean, focus: boolean): boolean =>
	hover || focus;
