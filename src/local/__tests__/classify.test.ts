import { createLocalApi } from '../api';
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
			'personas',
			'personas/abc-123',
			'notify-tokens',
			'notify-tokens/7',
			'user/alarm-phone',
			'user/alarm-phone/test-call',
			'persona-tokens',
			'persona-tokens/3',
			'smart-device/token/generate',
			'smart-device/token/revoke',
			'telegram/link/generate',
			'telegram/unlink',
			'user/store-avatar',
			'password/reset',
			'stats',
		]) {
			expect(classify('GET', url, true)).toBe('server');
		}
		expect(classify('POST', '/notify-tokens', true)).toBe('server');
		expect(classify('DELETE', '/notify-tokens/7', true)).toBe('server');
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
			['GET', 'https://api.tmgr.dev/api/tasks/5'],
		]) {
			expect(classify(method, url, true)).toBe('local');
		}
	});

	it('collects running timers from both sides and lets the tray stop a cloud timer', () => {
		expect(classify('GET', 'tasks/runned', true)).toBe('runned');
		expect(classify('DELETE', 'tasks/5/countdown', true, { workspace_id: 56 })).toBe('server');
		expect(classify('DELETE', 'tasks/5/countdown', true, { workspace_id: -42 })).toBe('local');
		expect(classify('DELETE', 'tasks/5/countdown', true)).toBe('local');
		expect(classify('PUT', 'tasks/5', true, { workspace_id: 56 })).toBe('local');
	});

	it('does not treat look-alike paths as account-level', () => {
		expect(classify('GET', 'users/5/tasks', true)).toBe('local');
		expect(classify('GET', 'user-stats', true)).toBe('local');
	});

	it('routes every daily-routines/* path to local, never to the server', () => {
		for (const [method, url] of [
			['GET', 'daily-routines/workspace'],
			['GET', 'daily-routines/tasks'],
			['GET', 'daily-routines/tasks/count'],
			['GET', 'daily-routines/tasks/archived/count'],
			['GET', 'daily-routines/tasks/completed/count'],
			['POST', 'daily-routines/tasks'],
			['POST', 'daily-routines/tasks/recurring'],
			['POST', 'daily-routines/tasks/quick'],
			['GET', 'daily-routines/tasks/1000000001'],
			['PUT', 'daily-routines/tasks/1000000001'],
			['DELETE', 'daily-routines/tasks/1000000001'],
			['POST', 'daily-routines/tasks/1000000001/complete'],
			['POST', 'daily-routines/tasks/1000000001/archive'],
			['POST', 'daily-routines/tasks/1000000001/complete-on'],
			['POST', 'daily-routines/tasks/1000000001/convert'],
			['GET', 'daily-routines/tasks/1000000001/instances'],
			['POST', 'daily-routines/tasks/1000000001/instances/1/complete'],
			['POST', 'daily-routines/tasks/1000000001/instances/1/skip'],
			['DELETE', 'daily-routines/tasks/1000000001/instances/1'],
			['PATCH', 'daily-routines/tasks/1000000001/instances/virtual'],
			['PUT', 'daily-routines/tasks/1000000001/pattern'],
			['GET', 'daily-routines/tasks/1000000001/stats'],
			['GET', 'daily-routines/tasks/upcoming'],
			['GET', 'daily-routines/expand'],
			['GET', 'daily-routines/expand/stats'],
			['GET', 'daily-routines/ics/import'],
			['GET', 'daily-routines/ics/export'],
		]) {
			expect(classify(method, url, true)).toBe('local');
			expect(classify(method, url, true)).not.toBe('server');
		}
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

describe('classify pages endpoints', () => {
	it('goes to the server outside a local workspace', () => {
		expect(classify('GET', 'pages/tree', false)).toBe('server');
		expect(classify('GET', 'tasks/5/pages', false)).toBe('server');
	});

	it('never reaches the server from a local workspace', () => {
		for (const url of ['pages', 'pages/tree', 'pages/trash', 'pages/search', 'pages/12/versions', 'pages/12/files', 'tasks/5/pages', 'workspaces/context']) {
			expect(classify('GET', url, true)).toBe('local');
		}
		for (const [method, url] of [
			['POST', 'pages'],
			['PATCH', 'pages/12'],
			['DELETE', 'pages/12'],
			['POST', 'pages/12/append'],
			['PUT', 'pages/12/sections/agent-notes'],
			['POST', 'pages/12/move'],
			['POST', 'pages/12/files'],
			['POST', 'pages/12/task-from-selection'],
			['POST', 'pages/12/follow'],
			['POST', 'pages/12/versions/2/restore'],
		]) {
			expect(classify(method, url, true)).toBe('local');
		}
	});

	it('keeps a page write that names another workspace from reaching the server', () => {
		expect(crossesWorkspaces('local', { workspace_id: 3 }, undefined, -1)).toBe(true);
	});
});

describe('pages endpoints in the local API', () => {
	const api = createLocalApi();

	it('are all answered by local routes, so nothing falls through to the server', () => {
		for (const [method, url] of [
			['GET', 'pages'],
			['GET', 'pages/tree'],
			['GET', 'pages/trash'],
			['GET', 'pages/search'],
			['POST', 'pages'],
			['GET', 'pages/12'],
			['PATCH', 'pages/12'],
			['DELETE', 'pages/12'],
			['POST', 'pages/12/append'],
			['PUT', 'pages/12/sections/s'],
			['POST', 'pages/12/move'],
			['POST', 'pages/12/pin'],
			['POST', 'pages/12/unpin'],
			['POST', 'pages/12/restore'],
			['GET', 'pages/12/versions'],
			['GET', 'pages/12/versions/3'],
			['POST', 'pages/12/versions/3/restore'],
			['GET', 'pages/12/backlinks'],
			['GET', 'pages/12/files'],
			['POST', 'pages/12/files'],
			['POST', 'pages/12/task-from-selection'],
			['POST', 'pages/12/follow'],
			['DELETE', 'pages/12/follow'],
			['GET', 'tasks/5/pages'],
		]) {
			expect(api.match(method, url)).not.toBeNull();
		}
	});
});
