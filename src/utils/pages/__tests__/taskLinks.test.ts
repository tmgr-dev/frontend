import { extractTaskLinkRefs, routeForTmgr } from '../taskLinks';
import { parseTmgrUrl } from '../tmgrLinks';

describe('extractTaskLinkRefs', () => {
	it('collects unique page and task refs from markdown and block json', () => {
		const refs = extractTaskLinkRefs(
			'a [A](tmgr://page/1) b [T](tmgr://task/2) [A](tmgr://page/1)',
			{ blocks: [{ data: { text: '<a href="tmgr://page/9">x</a>' } }] },
			null,
		);
		expect(refs).toEqual([
			{ kind: 'page', id: '1' },
			{ kind: 'task', id: '2' },
			{ kind: 'page', id: '9' },
		]);
	});

	it('ignores other kinds', () => {
		expect(
			extractTaskLinkRefs('[x](tmgr://user/1) [f](tmgr://file/2)'),
		).toEqual([]);
	});
});

describe('routeForTmgr', () => {
	const route = (url: string) => routeForTmgr(parseTmgrUrl(url)!, 'ws');

	it('routes pages by id, tasks, users and deep links', () => {
		expect(route('tmgr://page/5')).toBe('/ws/pages/5');
		expect(route('tmgr://task/7')).toBe('/ws/tasks/7');
		expect(route('tmgr://user/3')).toBe('/ws/team/3');
		expect(route('tmgr://page/other/notes')).toBe('/other/pages/notes');
	});

	it('has no route for files', () => {
		expect(route('tmgr://file/3')).toBeNull();
	});
});
