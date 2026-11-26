import type { TrustAlert } from './types';

export type ResetStorage = Pick<Storage, 'getItem' | 'setItem'>;

const KEY = 'manager.historyResetSeen';

const defaultStorage = (): ResetStorage | null => {
	try {
		return localStorage;
	} catch {
		return null;
	}
};

export function loadResetSeen(
	storage: ResetStorage | null = defaultStorage(),
): number {
	try {
		return Number(storage?.getItem(KEY)) || 0;
	} catch {
		return 0;
	}
}

export function saveResetSeen(
	at: number,
	storage: ResetStorage | null = defaultStorage(),
): void {
	try {
		storage?.setItem(KEY, String(at));
	} catch {
		/* not persisted */
	}
}

export function unseenResetAt(alerts: TrustAlert[], seen: number): number {
	const at = Math.max(
		0,
		...alerts.filter((a) => a.kind === 'history_reset').map((a) => a.at_ms),
	);
	return at > seen ? at : 0;
}
