import { routeViewKey } from '../routeViewKey';

describe('route view identity', () => {
	it.each([
		'WorkspaceBoard',
		'FallbackBoard',
		'WorkspaceTasksList',
		'ArchiveTasksList',
		'WorkspaceCategories',
		'WorkspaceCategoryChildren',
		'WorkspaceCategory',
		'WorkspaceMember',
	])('%s retains its view when only filters change', (name) => {
		const first = { name, path: '/demo/list', fullPath: '/demo/list?page=1' };
		expect(routeViewKey(first)).toBe(
			routeViewKey({ ...first, fullPath: '/demo/list?page=2&sort=created_at' }),
		);
	});
	it('recreates a view when the workspace or resource path changes', () => {
		expect(
			routeViewKey({
				name: 'WorkspaceBoard',
				path: '/a/board',
				fullPath: '/a/board',
			}),
		).not.toBe(
			routeViewKey({
				name: 'WorkspaceBoard',
				path: '/b/board',
				fullPath: '/b/board',
			}),
		);
	});
	it('keeps the member page across tab and window changes but not across members', () => {
		const member = (path: string, query = '') => ({
			name: 'WorkspaceMember',
			path,
			fullPath: path + query,
		});
		const key = routeViewKey(
			member('/spendly/team/1', '?window=30d&tab=touched'),
		);
		expect(
			routeViewKey(member('/spendly/team/1', '?window=30d&tab=assigned')),
		).toBe(key);
		expect(
			routeViewKey(member('/spendly/team/1', '?window=7d&tab=touched')),
		).toBe(key);
		expect(routeViewKey(member('/spendly/team/1', '?tab=bogus'))).toBe(key);
		expect(routeViewKey(member('/spendly/team/1'))).toBe(key);
		expect(
			routeViewKey(member('/spendly/team/2', '?window=30d&tab=touched')),
		).not.toBe(key);
	});
	it('preserves query-sensitive identity for screens that do not handle query updates yet', () => {
		expect(
			routeViewKey({
				name: 'WorkspaceCategoryEdit',
				path: '/a/category/1',
				fullPath: '/a/category/1?page=1',
			}),
		).not.toBe(
			routeViewKey({
				name: 'WorkspaceCategoryEdit',
				path: '/a/category/1',
				fullPath: '/a/category/1?page=2',
			}),
		);
	});
});
