import {
	buildTmgrLink,
	extractTmgrRefs,
	parseTmgrUrl,
	pathForRef,
} from '../tmgrLinks';

describe('parseTmgrUrl', () => {
	it.each([
		['tmgr://page/123', { form: 'storage', kind: 'page', id: '123' }],
		['tmgr://task/456', { form: 'storage', kind: 'task', id: '456' }],
		['tmgr://category/7', { form: 'storage', kind: 'category', id: '7' }],
		['tmgr://user/9', { form: 'storage', kind: 'user', id: '9' }],
		['tmgr://file/321', { form: 'storage', kind: 'file', id: '321' }],
		[
			'tmgr://persona/0b9e2c1a-3d4f-4a5b-8c6d-7e8f9a0b1c2d',
			{
				form: 'storage',
				kind: 'persona',
				id: '0b9e2c1a-3d4f-4a5b-8c6d-7e8f9a0b1c2d',
			},
		],
		['tmgr://persona/5', { form: 'storage', kind: 'persona', id: '5' }],
	])('storage form %s', (url, expected) => {
		expect(parseTmgrUrl(url)).toEqual(expected);
	});

	it('distinguishes the deep link tmgr://page/{ws}/{slug} from storage form', () => {
		expect(parseTmgrUrl('tmgr://page/tmgrdev/my-page')).toEqual({
			form: 'deep',
			kind: 'page',
			workspace: 'tmgrdev',
			slug: 'my-page',
		});
		expect(parseTmgrUrl('tmgr://page/123')?.form).toBe('storage');
	});

	it('a numeric-looking workspace segment still means deep when a slug follows', () => {
		expect(parseTmgrUrl('tmgr://page/ws1/p-123')?.form).toBe('deep');
	});

	it.each([
		'https://example.com',
		'tmgr://page/',
		'tmgr://page/abc',
		'tmgr://unknown/1',
		'tmgr://task/1/extra',
		'tmgr://file/12abc',
		'javascript:alert(1)',
		'',
	])('rejects %s', (url) => {
		expect(parseTmgrUrl(url)).toBeNull();
	});

	it('tolerates surrounding whitespace and a trailing slash-free query-less form only', () => {
		expect(parseTmgrUrl('  tmgr://page/1 ')).toMatchObject({ id: '1' });
		expect(parseTmgrUrl('tmgr://page/1?x=1')).toBeNull();
	});
});

describe('extractTmgrRefs', () => {
	it('collects unique storage refs from links and images', () => {
		const md =
			'[A](tmgr://page/1) and [B](tmgr://task/2) again [A2](tmgr://page/1)\n\n![](tmgr://file/3)';
		expect(extractTmgrRefs(md)).toEqual([
			{ kind: 'page', id: '1' },
			{ kind: 'task', id: '2' },
			{ kind: 'file', id: '3' },
		]);
	});

	it('skips refs inside fenced code and inline code', () => {
		const md =
			'```\n[A](tmgr://page/1)\n```\n`[B](tmgr://page/2)` [C](tmgr://page/3)';
		expect(extractTmgrRefs(md)).toEqual([{ kind: 'page', id: '3' }]);
	});

	it('ignores deep links', () => {
		expect(extractTmgrRefs('[A](tmgr://page/ws/slug)')).toEqual([]);
	});
});

describe('buildTmgrLink', () => {
	it('builds the storage form', () => {
		expect(buildTmgrLink('page', 12, 'Саша')).toBe('[Саша](tmgr://page/12)');
	});

	it('escapes brackets in the title', () => {
		expect(buildTmgrLink('task', 1, 'a [b] c')).toBe(
			'[a \\[b\\] c](tmgr://task/1)',
		);
	});
});

describe('pathForRef', () => {
	const ctx = {
		workspaceCode: 'ws',
		pageSlugs: { '5': 'five' } as Record<string, string>,
		categoryCodes: { '7': 'back' } as Record<string, string>,
	};

	it('routes a page by resolved slug', () => {
		expect(pathForRef({ kind: 'page', id: '5' }, ctx)).toBe('/ws/pages/five');
	});

	it('returns null for an unresolved page', () => {
		expect(pathForRef({ kind: 'page', id: '6' }, ctx)).toBeNull();
	});

	it('routes task, user, category, persona', () => {
		expect(pathForRef({ kind: 'task', id: '9' }, ctx)).toBe('/ws/tasks/9');
		expect(pathForRef({ kind: 'user', id: '3' }, ctx)).toBe('/ws/team/3');
		expect(pathForRef({ kind: 'category', id: '7' }, ctx)).toBe('/ws/back');
		expect(pathForRef({ kind: 'category', id: '8' }, ctx)).toBe(
			'/ws/categories',
		);
		expect(pathForRef({ kind: 'persona', id: 'u' }, ctx)).toBe(
			'/settings/personas',
		);
	});

	it('routes a deep link without lookup', () => {
		expect(
			pathForRef(
				{ form: 'deep', kind: 'page', workspace: 'other', slug: 's' },
				ctx,
			),
		).toBe('/other/pages/s');
	});

	it('files have no route', () => {
		expect(pathForRef({ kind: 'file', id: '1' }, ctx)).toBeNull();
	});
});
