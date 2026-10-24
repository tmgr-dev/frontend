import type { DesktopPlatform } from '@/utils/desktop';

export const BACKUP_STALE_DAYS = 3;

const BANNER_KEY = 'tmgr:local-only-banner';

export const welcomeNote = (platform: DesktopPlatform | null): string =>
	`Your data stays on this ${
		platform === 'macos' ? 'Mac' : 'computer'
	}. You can create an account later.`;

const plural = (n: number, unit: string) => `${n} ${unit}${n === 1 ? '' : 's'}`;

export const lastBackupLabel = (
	iso: string | null | undefined,
	now: number = Date.now(),
): string => {
	const at = iso ? Date.parse(iso) : NaN;
	if (Number.isNaN(at)) return 'No backup yet';
	const diff = Math.max(0, now - at);
	const minutes = Math.floor(diff / 60_000);
	if (minutes < 1) return 'Last backup: just now';
	if (minutes < 60) return `Last backup: ${plural(minutes, 'minute')} ago`;
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return `Last backup: ${plural(hours, 'hour')} ago`;
	return `Last backup: ${plural(Math.floor(hours / 24), 'day')} ago`;
};

export interface BackupWarning {
	kind: 'failed' | 'stale';
	message: string;
}

export const backupWarning = (
	status: { last_backup_at: string | null; last_error: string | null },
	now: number = Date.now(),
): BackupWarning | null => {
	if (status.last_error) {
		return {
			kind: 'failed',
			message: `Last backup failed: ${status.last_error}`,
		};
	}
	const at = status.last_backup_at ? Date.parse(status.last_backup_at) : NaN;
	if (Number.isNaN(at)) return null;
	const days = Math.floor((now - at) / 86_400_000);
	if (days < BACKUP_STALE_DAYS) return null;
	return { kind: 'stale', message: `No backup for ${days} days` };
};

export const settingsAccess = (localOnly: boolean) => ({
	profileCloud: !localOnly,
	profileLocal: localOnly,
	account: localOnly,
	security: !localOnly,
	notifications: !localOnly,
	device: !localOnly,
	telegram: !localOnly,
	alarmPhone: !localOnly,
});

export const moduleNeedsAccount = (
	entry: Record<string, unknown> | null | undefined,
): boolean =>
	!!entry && (entry.requires_account === true || entry.local === false);

export type GateMode =
	'requires-account' | 'not-in-local-workspace' | 'default';

export const gateMode = (input: {
	entry: Record<string, unknown> | null | undefined;
	localOnly: boolean;
	localWorkspace: boolean;
}): GateMode => {
	if (!moduleNeedsAccount(input.entry)) return 'default';
	if (input.localOnly) return 'requires-account';
	if (input.localWorkspace) return 'not-in-local-workspace';
	return 'default';
};

export const bannerDismissed = (): boolean => {
	try {
		return localStorage.getItem(BANNER_KEY) === '1';
	} catch {
		return false;
	}
};

export const dismissBanner = (): void => {
	try {
		localStorage.setItem(BANNER_KEY, '1');
	} catch {
		return;
	}
};

export const visiblePresets = <T extends { key: string }>(
	presets: T[],
	localOnly: boolean,
): T[] => (localOnly ? presets.filter((p) => p.key !== 'team') : presets);

export const tildePath = (path: string): string =>
	path.replace(
		/^(?:\/Users\/[^/]+|\/home\/[^/]+|[A-Za-z]:\\Users\\[^\\]+)(?=[/\\]|$)/,
		'~',
	);
