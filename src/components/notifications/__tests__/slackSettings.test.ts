import { readFileSync } from 'fs';
import { join } from 'path';
import {
	SLACK_DM_TYPES,
	slackPayload,
	slackSectionState,
	slackTypeChecked,
	toggleSlackType,
} from '../slackSettings';

describe('slackSectionState', () => {
	const on = { installed: true, linked: true };

	it('shows the toggles only for an installed and linked user with the module on', () => {
		expect(slackSectionState(true, on)).toBe('settings');
	});

	it('hints when installed but not linked', () => {
		expect(slackSectionState(true, { installed: true, linked: false })).toBe(
			'link',
		);
	});

	it('hides when the module is off, not installed or unknown', () => {
		expect(slackSectionState(false, on)).toBe('hidden');
		expect(slackSectionState(true, { installed: false, linked: false })).toBe(
			'hidden',
		);
		expect(slackSectionState(true, null)).toBe('hidden');
	});
});

describe('slack types', () => {
	it('renders null as every type checked', () => {
		for (const type of SLACK_DM_TYPES)
			expect(slackTypeChecked(null, type)).toBe(true);
		expect(slackTypeChecked(['task_assigned'], 'page.mentioned')).toBe(false);
	});

	it('turns null into the full explicit list minus the unchecked type', () => {
		expect(toggleSlackType(null, 'page.mentioned', false)).toEqual([
			'task_assigned',
		]);
		expect(toggleSlackType(['task_assigned'], 'page.mentioned', true)).toEqual([
			'task_assigned',
			'page.mentioned',
		]);
	});
});

describe('slackPayload', () => {
	it('omits untouched null fields', () => {
		expect(slackPayload(null, null)).toEqual({});
	});

	it('sends explicit values', () => {
		expect(slackPayload(false, ['task_assigned'])).toEqual({
			slack_enabled: false,
			slack_types: ['task_assigned'],
		});
	});
});

describe('NotificationSettingsForm slack section', () => {
	const source = readFileSync(
		join(__dirname, '..', 'NotificationSettingsForm.vue'),
		'utf8',
	);

	it('is driven by the module, the installation and the section state', () => {
		expect(source).toContain('getSlackInstallation');
		expect(source).toContain("slackSectionState(isFeatureEnabled('slack')");
		expect(source).toContain('/settings/integrations/slack');
		expect(source).toContain('formData.slack_enabled');
		expect(source).toContain('...slackPayload(');
	});
});
