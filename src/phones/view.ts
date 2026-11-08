import type { PairingView } from './types';

export const DEFAULT_DESKTOP_NAME = 'This Mac';

export const phonesAvailable = (
	desktop: boolean,
	localOnly: boolean,
): boolean => desktop && !localOnly;

export const shortFingerprint = (fingerprint: string): string =>
	fingerprint.slice(0, 8).toUpperCase();

export const countdownLabel = (expiresMs: number, now: number): string => {
	const left = Math.max(0, Math.ceil((expiresMs - now) / 1000));
	const mm = Math.floor(left / 60);
	const ss = String(left % 60).padStart(2, '0');
	return `${mm}:${ss}`;
};

export const isExpired = (expiresMs: number, now: number): boolean =>
	now >= expiresMs;

export const sinceLabel = (ms: number, now: number): string => {
	if (!ms) return 'never';
	const diff = Math.max(0, now - ms);
	const minutes = Math.floor(diff / 60_000);
	if (minutes < 1) return 'just now';
	if (minutes < 60) return `${minutes} min ago`;
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return `${hours} h ago`;
	const days = Math.floor(hours / 24);
	return `${days} d ago`;
};

export const pairingHeadline = (pairing: PairingView): string => {
	switch (pairing.state) {
		case 'waiting':
			return 'Scan this code with the TMGR iPhone app';
		case 'confirm':
			return pairing.phone_name
				? `Type the code shown on ${pairing.phone_name}`
				: 'Type the code shown on your iPhone';
		case 'failed':
			return pairing.error || 'Pairing failed';
		case 'done':
			return pairing.phone_name
				? `${pairing.phone_name} is paired`
				: 'Phone paired';
	}
};

export const attemptsLabel = (left: number): string =>
	`${left} ${left === 1 ? 'attempt' : 'attempts'} left`;
