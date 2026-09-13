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
