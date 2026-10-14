import { scrollBehavior } from '../memberPageScroll';

const member = (query: Record<string, string> = {}, path = '/ws/team/2') => ({
	name: 'WorkspaceMember',
	path,
	hash: '',
	query,
});
const TOP = { left: 0, top: 0 };

describe('scrollBehavior', () => {
	it.each([null, { left: 0, top: 500 }])(
		'keeps the scroll for a tab or period change on the same member (savedPosition %p)',
		(saved) => {
			expect(
				scrollBehavior(
					member({ tab: 'assigned', window: '7d' }),
					member({ tab: 'touched', window: '30d' }),
					saved,
				),
			).toBe(false);
			expect(scrollBehavior(member({ window: '7d' }), member(), saved)).toBe(
				false,
			);
		},
	);

	it('scrolls to top for another member', () => {
		expect(
			scrollBehavior(member({}, '/ws/team/3'), member({}, '/ws/team/2')),
		).toEqual(TOP);
	});

	it('scrolls to top for another route', () => {
		expect(
			scrollBehavior(
				{ name: 'Settings', path: '/settings', hash: '', query: {} },
				member(),
			),
		).toEqual(TOP);
	});

	it('scrolls to top when another query param changes', () => {
		expect(
			scrollBehavior(
				member({ tab: 'touched', page: '2' }),
				member({ tab: 'touched' }),
			),
		).toEqual(TOP);
	});

	it('scrolls to top for a Settings tab change', () => {
		const settings = (tab: string) => ({
			name: 'Settings',
			path: '/settings',
			hash: '',
			query: { tab },
		});
		expect(
			scrollBehavior(settings('notification'), settings('profile')),
		).toEqual(TOP);
	});

	it.each([
		[{ page: '2' }, { page: '1' }],
		[{ categories_page: '2' }, {}],
		[{ tab: 'x', window: '7d' }, {}],
	])(
		'scrolls to top for a category list query change %p',
		(toQuery, fromQuery) => {
			const category = (query: Record<string, string>) => ({
				name: 'WorkspaceCategory',
				path: '/ws/proj',
				hash: '',
				query,
			});
			expect(scrollBehavior(category(toQuery), category(fromQuery))).toEqual(
				TOP,
			);
		},
	);

	it('scrolls to top when the hash changes on the same member', () => {
		expect(
			scrollBehavior({ ...member({ tab: 'assigned' }), hash: '#a' }, member()),
		).toEqual(TOP);
	});

	it('scrolls to top on initial navigation', () => {
		expect(
			scrollBehavior(member(), {
				name: undefined,
				path: '/',
				hash: '',
				query: {},
			}),
		).toEqual(TOP);
	});
});
