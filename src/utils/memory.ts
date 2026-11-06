import type { MemoryEntry } from '@/actions/tmgr/memory';

export const UNDO_WINDOW_MS = 10000;
export const MAX_FACT_LENGTH = 300;

export const secondsLeft = (
	autoAddAt: string | null,
	serverNow: number,
): number | null => {
	if (!autoAddAt) return null;
	const target = Date.parse(autoAddAt);
	if (Number.isNaN(target)) return null;
	return Math.max(0, Math.ceil((target - serverNow) / 1000));
};

export const formatCountdown = (seconds: number): string => {
	const minutes = Math.floor(seconds / 60);
	const rest = String(seconds % 60).padStart(2, '0');
	return `${minutes}:${rest}`;
};

export const newestFirst = (entries: MemoryEntry[]): MemoryEntry[] =>
	[...entries].reverse();

const TASK_KEY = /^[A-Z][A-Z0-9]*-\d+$/;

export const looksLikeTaskKey = (source: string | null | undefined): boolean =>
	!!source && TASK_KEY.test(source);
