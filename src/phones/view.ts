import { t } from '@/i18n';
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
	if (!ms) return t('settingsPages.helpers.phones.never');
	const diff = Math.max(0, now - ms);
	const minutes = Math.floor(diff / 60_000);
	if (minutes < 1) return t('settingsPages.helpers.phones.justNow');
	if (minutes < 60)
		return t('settingsPages.helpers.phones.minutesAgo', { n: minutes });
	const hours = Math.floor(minutes / 60);
	if (hours < 24)
		return t('settingsPages.helpers.phones.hoursAgo', { n: hours });
	const days = Math.floor(hours / 24);
	return t('settingsPages.helpers.phones.daysAgo', { n: days });
};

export const pairingHeadline = (pairing: PairingView): string => {
	switch (pairing.state) {
		case 'waiting':
			return t('settingsPages.helpers.phones.scanCode');
		case 'confirm':
			return pairing.phone_name
				? t('settingsPages.helpers.phones.typeCodeOn', {
						name: pairing.phone_name,
					})
				: t('settingsPages.helpers.phones.typeCodeOnIphone');
		case 'failed':
			return pairing.error || t('settingsPages.helpers.phones.pairingFailed');
		case 'done':
			return pairing.phone_name
				? t('settingsPages.helpers.phones.isPaired', {
						name: pairing.phone_name,
					})
				: t('settingsPages.helpers.phones.phonePaired');
	}
};

export const attemptsLabel = (left: number): string =>
	t('settingsPages.helpers.phones.attemptsLeft', left);
