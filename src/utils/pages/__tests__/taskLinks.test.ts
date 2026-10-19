import {
	extractTaskLinkRefs,
	routeForTmgr,
	taskIdForModal,
} from '../taskLinks';
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

describe('taskIdForModal', () => {
	const id = (url: string, detached = false) =>
		taskIdForModal(parseTmgrUrl(url)!, detached);

	it('opens task links in the modal', () => {
		expect(id('tmgr://task/7')).toBe(7);
	});

	it('leaves pages, other kinds and detached windows to the router', () => {
		expect(id('tmgr://page/5')).toBeNull();
		expect(id('tmgr://page/other/notes')).toBeNull();
		expect(id('tmgr://user/3')).toBeNull();
		expect(id('tmgr://task/7', true)).toBeNull();
	});
});
