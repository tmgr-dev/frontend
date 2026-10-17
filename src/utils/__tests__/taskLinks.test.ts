import {
	buildAppTaskUrl,
	buildMarkdownLink,
	buildWebTaskUrl,
	parseTaskLink,
	taskShareLinks,
} from '../taskLinks';

describe('building links', () => {
	it('builds the web url on the production origin', () => {
		expect(buildWebTaskUrl('tmgrdev', 8974)).toBe(
			'https://tmgr.dev/tmgrdev/tasks/8974',
		);
	});

	it('builds the app url with the workspace', () => {
		expect(buildAppTaskUrl('tmgrdev', 8974)).toBe('tmgr://task/tmgrdev/8974');
	});
});

describe('buildMarkdownLink', () => {
	const url = 'https://tmgr.dev/w/tasks/1';

	it('does not double the key already in the title', () => {
		expect(buildMarkdownLink('TM-123: Title', url)).toBe(
			`[TM-123: Title](${url})`,
		);
	});

	it('keeps a title without a key as the label', () => {
		expect(buildMarkdownLink('Title', url)).toBe(`[Title](${url})`);
	});

	it('falls back to a generic label for an empty title', () => {
		expect(buildMarkdownLink('  ', url)).toBe(`[Task](${url})`);
		expect(buildMarkdownLink(undefined, url)).toBe(`[Task](${url})`);
	});

	it('escapes characters that would break the link', () => {
		expect(buildMarkdownLink('TM-1: a [b] c', url)).toBe(
			`[TM-1: a \\[b\\] c](${url})`,
		);
	});

	it('collapses newlines in the label', () => {
		expect(buildMarkdownLink('a\nb', url)).toBe(`[a b](${url})`);
	});
});

describe('parseTaskLink', () => {
	it('reads the legacy app form without a workspace', () => {
		expect(parseTaskLink('tmgr://task/9233')).toEqual({
			workspaceCode: null,
			taskId: 9233,
		});
		expect(parseTaskLink(' tmgr://task/9233/ ')).toEqual({
			workspaceCode: null,
			taskId: 9233,
		});
	});

	it('reads the workspace app form', () => {
		expect(parseTaskLink('tmgr://task/my_ws-1/9233')).toEqual({
			workspaceCode: 'my_ws-1',
			taskId: 9233,
		});
	});

	it('reads the web form, ignoring query and hash', () => {
		const expected = { workspaceCode: 'tmgrdev', taskId: 8974 };
		expect(parseTaskLink('https://tmgr.dev/tmgrdev/tasks/8974')).toEqual(
			expected,
		);
		expect(parseTaskLink('https://tmgr.dev/tmgrdev/tasks/8974/')).toEqual(
			expected,
		);
		expect(parseTaskLink('https://tmgr.dev/tmgrdev/tasks/8974?a=1#c')).toEqual(
			expected,
		);
	});

	it('round-trips the builders', () => {
		expect(parseTaskLink(buildWebTaskUrl('w', 5))).toEqual({
			workspaceCode: 'w',
			taskId: 5,
		});
		expect(parseTaskLink(buildAppTaskUrl('w', 5))).toEqual({
			workspaceCode: 'w',
			taskId: 5,
		});
	});

	it('rejects look-alikes and malformed links', () => {
		for (const bad of [
			'tmgr://task/abc',
			'tmgr://task/',
			'tmgr://task/w/x',
			'tmgr://task/w/1/2',
			'tmgr://task/w%2Fx/1',
			'tmgr://task/0',
			'tmgr://page/w/1',
			'http://tmgr.dev/w/tasks/1',
			'https://tmgr.dev.evil.com/w/tasks/1',
			'https://evil.com/tmgr.dev/w/tasks/1',
			'https://user@tmgr.dev/w/tasks/1',
			'https://tmgr.dev/w/tasks/abc',
			'https://tmgr.dev/w/task/1',
			'https://tmgr.dev/tasks/1',
			'https://tmgr.dev/w/tasks/1/extra',
			'https://tmgr.dev:8443/w/tasks/1',
			'https://tmgr.dev/w/tasks/99999999999999999999',
			'',
		]) {
			expect(parseTaskLink(bad)).toBeNull();
		}
	});
});

describe('taskShareLinks', () => {
	it('builds web, app and markdown links for a synced workspace', () => {
		expect(
			taskShareLinks(
				{ id: 8, title: 'TM-8: Share' },
				{ id: 2, code: 'tmgrdev' },
			),
		).toEqual({
			web: 'https://tmgr.dev/tmgrdev/tasks/8',
			app: 'tmgr://task/tmgrdev/8',
			markdown: '[TM-8: Share](https://tmgr.dev/tmgrdev/tasks/8)',
		});
	});

	it('has no web link for a local workspace and points markdown at the app link', () => {
		expect(
			taskShareLinks({ id: 8, title: 'x' }, { id: -1, code: 'local' }),
		).toEqual({
			web: null,
			app: 'tmgr://task/local/8',
			markdown: '[x](tmgr://task/local/8)',
		});
	});
});
