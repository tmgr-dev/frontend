import { classify, crossesWorkspaces } from '../classify';

describe('classify', () => {
	it('sends everything to the server outside a local workspace, except the workspace list and switch', () => {
		expect(classify('GET', 'tasks/current', false)).toBe('server');
		expect(classify('get', '/workspaces', false)).toBe('server:workspaces');
		expect(classify('PUT', 'v2/user/settings', false)).toBe('settings');
		expect(classify('GET', 'user', false)).toBe('server:user');
	});

	it('keeps account-level calls on the server inside a local workspace', () => {
		for (const url of [
			'user/settings',
			'user/feature-toggles',
			'auth/refresh',
			'notifications/unread-count',
			'notification-settings',
			'/broadcasting/auth',
			'v2/user/settings',
		]) {
			expect(classify('GET', url, true)).toBe('server');
		}
		expect(classify('GET', 'user', true)).toBe('server:user');
	});

	it('never sends workspace data to the server inside a local workspace', () => {
		for (const [method, url] of [
			['GET', 'tasks/current?page=1'],
			['POST', '/tasks'],
			['PUT', 'tasks/5'],
			['GET', 'tasks/5/files'],
			['POST', 'files/presign-upload'],
			['POST', 'agent/conversations'],
			['POST', 'tasks/5/comments/help'],
			['GET', 'workspaces/-42/members'],
			['GET', 'dashboard/statistics'],
			['GET', 'daily-routines/tasks'],
			['GET', 'https://api.tmgr.dev/api/tasks/runned'],
		]) {
			expect(classify(method, url, true)).toBe('local');
		}
	});

	it('does not treat look-alike paths as account-level', () => {
		expect(classify('GET', 'users/5/tasks', true)).toBe('local');
		expect(classify('GET', 'user-stats', true)).toBe('local');
	});
});

describe('crossesWorkspaces', () => {
	it('refuses sending a local task to the server', () => {
		expect(crossesWorkspaces('server', { title: 'x', workspace_id: -42 }, undefined, null)).toBe(true);
		expect(crossesWorkspaces('server', JSON.stringify({ workspace_id: -42 }), undefined, null)).toBe(true);
		expect(crossesWorkspaces('server', {}, { workspace_id: -42 }, null)).toBe(true);
		expect(crossesWorkspaces('server', { workspace_id: 56 }, undefined, null)).toBe(false);
	});

	it('refuses writing cloud or other-local data into the active local workspace', () => {
		expect(crossesWorkspaces('local', { workspace_id: 56 }, undefined, -42)).toBe(true);
		expect(crossesWorkspaces('local', { workspace_id: -7 }, undefined, -42)).toBe(true);
		expect(crossesWorkspaces('local', { workspace_id: -42 }, undefined, -42)).toBe(false);
		expect(crossesWorkspaces('local', { title: 'no workspace' }, undefined, -42)).toBe(false);
	});
});
