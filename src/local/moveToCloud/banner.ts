import type { PublishedMove } from './model';

export const formatMoveDate = (iso: string): string =>
	new Date(iso).toLocaleDateString('en-US', {
		dateStyle: 'medium',
		timeZone: 'UTC',
	});

export const copiedBannerText = (
	published: Pick<PublishedMove, 'name' | 'at'>,
): string =>
	`Copied to your cloud workspace ${published.name} on ${formatMoveDate(
		published.at,
	)}. Changes here are not synced.`;

export const previousMoveWarning = (
	published: Pick<PublishedMove, 'at'>,
): string =>
	`You moved this workspace on ${formatMoveDate(
		published.at,
	)}. This creates a second cloud copy.`;

const dismissed = new Set<string>();

export const bannerDismissedThisSession = (code: string) => dismissed.has(code);

export const dismissBannerForSession = (code: string) => {
	dismissed.add(code);
};

export const formatBytes = (bytes: number): string => {
	if (bytes < 1024) return `${bytes} B`;
	const units = ['KB', 'MB', 'GB'];
	let value = bytes / 1024;
	let unit = 0;
	while (value >= 1024 && unit < units.length - 1) {
		value /= 1024;
		unit++;
	}
	return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`;
};
