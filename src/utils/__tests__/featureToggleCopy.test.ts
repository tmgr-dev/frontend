import {
	featureDescription,
	humanizeGroupName,
	humanizeKey,
} from '../featureToggleCopy';

describe('humanizeKey', () => {
	it('humanizes a dotted feature key', () => {
		expect(humanizeKey('task.countdown')).toBe('Task countdown');
	});

	it('humanizes an underscored key', () => {
		expect(humanizeKey('default_landing_page')).toBe('Default landing page');
	});

	it('leaves a plain single-word key capitalized', () => {
		expect(humanizeKey('board')).toBe('Board');
	});
});

describe('humanizeGroupName', () => {
	it('maps known groups to their display name', () => {
		expect(humanizeGroupName('pages')).toBe('Pages');
		expect(humanizeGroupName('task')).toBe('Tasks');
		expect(humanizeGroupName('notifications')).toBe('Notifications');
	});

	it('falls back to a humanized key for unknown groups', () => {
		expect(humanizeGroupName('other')).toBe('Other');
	});
});

describe('featureDescription', () => {
	it('prefers a backend-provided description when present', () => {
		expect(featureDescription('board', { description: 'Custom copy' })).toBe(
			'Custom copy',
		);
	});

	it('falls back to the copy map for known keys', () => {
		expect(featureDescription('task.countdown')).toMatch(/countdown/i);
	});

	it('returns an empty string for unknown keys', () => {
		expect(featureDescription('unknown.key')).toBe('');
	});
});
