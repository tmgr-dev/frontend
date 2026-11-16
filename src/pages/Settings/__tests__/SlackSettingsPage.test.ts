import { readFileSync } from 'fs';
import { join } from 'path';

const read = (...parts: string[]) =>
	readFileSync(join(__dirname, '..', '..', '..', ...parts), 'utf8');

describe('slack route', () => {
	const routes = read('router', 'routes.js');

	it('is gated by the slack module and keeps the query across login', () => {
		const block = routes.slice(routes.indexOf('/settings/integrations/slack'));
		const route = block.slice(0, block.indexOf('name:'));
		expect(route).toContain("gatedPage(\n\t\t\t'slack'");
		expect(route).toContain('returnAfterLogin: true');
		expect(route).toContain('settingsLayout: true');
	});
});

describe('SlackSettingsPage', () => {
	const source = read('pages', 'Settings', 'SlackSettingsPage.vue');

	it('handles the oauth redirect and strips the query', () => {
		expect(source).toContain('parseSlackRedirect(route.query)');
		expect(source).toContain('confirmSlackInstall');
		expect(source).toContain('confirmSlackLink');
		expect(source).toContain('router.replace');
		expect(source).toContain('slackConfirmFailureKey');
		expect(source).toContain('slackReasonKey');
	});

	it('limits connect, disconnect and admin sections to admins', () => {
		expect(source).toContain('canManage');
		expect(source).toContain('v-if="canManage"');
		expect(source).toContain('settings.slack.askAdmin');
		expect(source).toContain('<SlackLinkedPeople');
		expect(source).toContain('<SlackChannelsPanel');
	});

	it('redirects to the install url and asks before disconnecting', () => {
		expect(source).toContain('window.location.href');
		expect(source).toContain('<AlertDialog');
	});
});

describe('SlackLinkedPeople', () => {
	const source = read('components', 'settings', 'SlackLinkedPeople.vue');

	it('lists links and unlinks a user', () => {
		expect(source).toContain('listSlackLinks');
		expect(source).toContain('unlinkSlackUser');
	});
});

describe('SlackChannelsPanel', () => {
	const source = read('components', 'settings', 'SlackChannelsPanel.vue');

	it('manages channels with the invite hint and shared badge', () => {
		expect(source).toContain('addSlackChannel');
		expect(source).toContain('removeSlackChannel');
		expect(source).toContain('settings.slack.channels.inviteHint');
		expect(source).toContain('settings.slack.channels.shared');
	});

	it('manages subscriptions', () => {
		expect(source).toContain('listSlackSubscriptions');
		expect(source).toContain('createSlackSubscription');
		expect(source).toContain('deleteSlackSubscription');
		expect(source).toContain("'task_created'");
		expect(source).toContain("'comment_created'");
		expect(source).toContain('settings.slack.subscriptions.wholeWorkspace');
		expect(source).toContain('settings.slack.subscriptions.anyPriority');
	});
});
