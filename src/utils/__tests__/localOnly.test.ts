import {
	backupWarning,
	bannerDismissed,
	dismissBanner,
	gateMode,
	lastBackupLabel,
	moduleNeedsAccount,
	settingsAccess,
	tildePath,
	visiblePresets,
	welcomeNote,
} from '../localOnly';

const NOW = Date.parse('2026-10-11T12:00:00Z');
const DAY = 86_400_000;
const ago = (ms: number) => new Date(NOW - ms).toISOString();

describe('welcomeNote', () => {
	it('names the Mac on macOS', () => {
		expect(welcomeNote('macos')).toBe(
			'Your data stays on this Mac. You can create an account later.',
		);
	});
	it('says computer elsewhere', () => {
		expect(welcomeNote('windows')).toBe(
			'Your data stays on this computer. You can create an account later.',
		);
		expect(welcomeNote('linux')).toContain('this computer');
	});
});

describe('lastBackupLabel', () => {
	it('says no backup yet', () => {
		expect(lastBackupLabel(null, NOW)).toBe('No backup yet');
		expect(lastBackupLabel('garbage', NOW)).toBe('No backup yet');
	});
	it('formats relative time', () => {
		expect(lastBackupLabel(ago(20_000), NOW)).toBe('Last backup: just now');
		expect(lastBackupLabel(ago(5 * 60_000), NOW)).toBe(
			'Last backup: 5 minutes ago',
		);
		expect(lastBackupLabel(ago(60_000 * 61), NOW)).toBe(
			'Last backup: 1 hour ago',
		);
		expect(lastBackupLabel(ago(3 * 3_600_000), NOW)).toBe(
			'Last backup: 3 hours ago',
		);
		expect(lastBackupLabel(ago(DAY), NOW)).toBe('Last backup: 1 day ago');
		expect(lastBackupLabel(ago(4 * DAY), NOW)).toBe('Last backup: 4 days ago');
	});
});

describe('backupWarning', () => {
	it('is quiet for a fresh backup', () => {
		expect(
			backupWarning({ last_backup_at: ago(DAY), last_error: null }, NOW),
		).toBeNull();
	});
	it('warns after three days', () => {
		expect(
			backupWarning({ last_backup_at: ago(3 * DAY), last_error: null }, NOW),
		).toEqual({ kind: 'stale', message: 'No backup for 3 days' });
		expect(
			backupWarning(
				{ last_backup_at: ago(5 * DAY + 3600_000), last_error: null },
				NOW,
			)?.message,
		).toBe('No backup for 5 days');
	});
	it('does not warn just under three days', () => {
		expect(
			backupWarning(
				{ last_backup_at: ago(3 * DAY - 60_000), last_error: null },
				NOW,
			),
		).toBeNull();
	});
	it('reports the last error first', () => {
		expect(
			backupWarning({ last_backup_at: ago(DAY), last_error: 'disk full' }, NOW),
		).toEqual({ kind: 'failed', message: 'Last backup failed: disk full' });
	});
	it('stays quiet for a workspace that never had a backup yet and is new', () => {
		expect(
			backupWarning({ last_backup_at: null, last_error: null }, NOW),
		).toBeNull();
	});
});

describe('settingsAccess', () => {
	it('shows everything for a cloud user', () => {
		expect(settingsAccess(false)).toEqual({
			profileCloud: true,
			profileLocal: false,
			account: false,
			security: true,
			notifications: true,
			device: true,
			telegram: true,
			alarmPhone: true,
		});
	});
	it('hides cloud sections in local-only mode', () => {
		expect(settingsAccess(true)).toEqual({
			profileCloud: false,
			profileLocal: true,
			account: true,
			security: false,
			notifications: false,
			device: false,
			telegram: false,
			alarmPhone: false,
		});
	});
});

describe('moduleNeedsAccount', () => {
	it('detects cloud-only modules', () => {
		expect(moduleNeedsAccount({ requires_account: true })).toBe(true);
		expect(moduleNeedsAccount({ local: false })).toBe(true);
		expect(moduleNeedsAccount({ local: true })).toBe(false);
		expect(moduleNeedsAccount({})).toBe(false);
		expect(moduleNeedsAccount(null)).toBe(false);
	});
});

describe('gateMode', () => {
	const cloudOnly = { requires_account: true, local: false };
	it('asks for an account in local-only mode', () => {
		expect(
			gateMode({ entry: cloudOnly, localOnly: true, localWorkspace: true }),
		).toBe('requires-account');
	});
	it('says not available in a local workspace for a signed-in user', () => {
		expect(
			gateMode({ entry: cloudOnly, localOnly: false, localWorkspace: true }),
		).toBe('not-in-local-workspace');
	});
	it('falls through to the usual gate otherwise', () => {
		expect(
			gateMode({ entry: cloudOnly, localOnly: false, localWorkspace: false }),
		).toBe('default');
		expect(
			gateMode({
				entry: { local: true },
				localOnly: true,
				localWorkspace: true,
			}),
		).toBe('default');
		expect(
			gateMode({ entry: undefined, localOnly: true, localWorkspace: true }),
		).toBe('default');
	});
});

describe('banner dismissal', () => {
	beforeEach(() => {
		const store = new Map<string, string>();
		(globalThis as any).localStorage = {
			getItem: (k: string) => store.get(k) ?? null,
			setItem: (k: string, v: string) => void store.set(k, v),
		};
	});
	it('remembers dismissal', () => {
		expect(bannerDismissed()).toBe(false);
		dismissBanner();
		expect(bannerDismissed()).toBe(true);
	});
});

describe('visiblePresets', () => {
	const presets = [{ key: 'solo' }, { key: 'team' }];
	it('hides the team preset in local-only mode', () => {
		expect(visiblePresets(presets, true)).toEqual([{ key: 'solo' }]);
		expect(visiblePresets(presets, false)).toEqual(presets);
	});
});

describe('tildePath', () => {
	it('replaces the home directory with a tilde', () => {
		expect(tildePath('/Users/ada/.tmgr.dev/workspaces/w/backups')).toBe(
			'~/.tmgr.dev/workspaces/w/backups',
		);
		expect(tildePath('/home/ada/.tmgr.dev')).toBe('~/.tmgr.dev');
		expect(tildePath('/var/data')).toBe('/var/data');
		expect(tildePath('')).toBe('');
	});
});
