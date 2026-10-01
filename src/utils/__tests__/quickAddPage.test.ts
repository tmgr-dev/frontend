import { crossesWorkspaces } from '@/local/classify';
import type { PageSummary } from '@/actions/tmgr/pages';
import {
	appendToLoadedPage,
	composeAppendMarkdown,
	filterPageOptions,
	flattenPages,
	listPageSections,
	pickRememberedPage,
	readLastPage,
	writeLastPage,
} from '../quickAddPage';

const page = (id: number, title: string, parent_id: number | null = null, position = 0): PageSummary => ({
	id,
	title,
	slug: `p${id}`,
	type: 'plain',
	parent_id,
	position,
	pinned: false,
	updated_at: '',
});

describe('listPageSections', () => {
	it('lists level-2 headings only, in order, with raw text', () => {
		const body = '# Title\n\nintro\n\n## Хронология\n- a\n### Sub\n## **Итоги** ##\n#### deep\n';
		expect(listPageSections(body)).toEqual(['Хронология', '**Итоги**']);
	});

	it('skips headings inside code fences and repeated headings', () => {
		const body = '```\n## fake\n```\n## Real\n## real\n~~~\n## also fake\n~~~\n';
		expect(listPageSections(body)).toEqual(['Real']);
	});

	it('is empty for an empty body', () => {
		expect(listPageSections('')).toEqual([]);
	});

	it('leaves out headings inside system sections', () => {
		const body =
			'## Кратко\n\n<!-- tmgr:section id="promises" owner="system" -->\n## Обещания\n- a\n<!-- /tmgr:section -->\n\n<!-- tmgr:section id="n" owner="agents" -->\n## Notes\n<!-- /tmgr:section -->\n\n## Хронология\n';
		expect(listPageSections(body)).toEqual(['Кратко', 'Notes', 'Хронология']);
	});

	it('stays linear on a 1 MB adversarial heading line', () => {
		for (const body of [
			`##${' '.repeat(1_000_000)}x y`,
			`## ${'a #'.repeat(350_000)}`,
			`##\t${'# '.repeat(500_000)}\u0001`,
		]) {
			const started = performance.now();
			listPageSections(body);
			expect(performance.now() - started).toBeLessThan(200);
		}
	});
});

describe('composeAppendMarkdown', () => {
	it('puts the screenshot link first, then the text', () => {
		expect(composeAppendMarkdown('  note  ', 12)).toBe('![](tmgr://file/12)\n\nnote');
	});

	it('handles text only and file only', () => {
		expect(composeAppendMarkdown('note', null)).toBe('note');
		expect(composeAppendMarkdown('  ', 5)).toBe('![](tmgr://file/5)');
		expect(composeAppendMarkdown('', null)).toBe('');
	});
});

describe('page options', () => {
	const pages = [page(1, 'Root B', null, 1), page(2, 'Root A', null, 0), page(3, 'Child', 2), page(4, 'Deep', 3)];

	it('flattens the tree depth-first with depth', () => {
		expect(flattenPages(pages)).toEqual([
			{ id: 2, title: 'Root A', depth: 0 },
			{ id: 3, title: 'Child', depth: 1 },
			{ id: 4, title: 'Deep', depth: 2 },
			{ id: 1, title: 'Root B', depth: 0 },
		]);
	});

	it('filters by title and keeps the selected page visible', () => {
		const options = flattenPages(pages);
		expect(filterPageOptions(options, 'root').map((o) => o.id)).toEqual([2, 1]);
		expect(filterPageOptions(options, 'root', 4).map((o) => o.id)).toEqual([2, 4, 1]);
		expect(filterPageOptions(options, '  ')).toBe(options);
	});

	it('remembers the page only while it still exists', () => {
		const options = flattenPages(pages);
		expect(pickRememberedPage(options, 3)).toBe(3);
		expect(pickRememberedPage(options, 99)).toBeNull();
		expect(pickRememberedPage(options, null)).toBeNull();
	});
});

describe('last page storage', () => {
	it('round-trips per workspace and tolerates missing storage', () => {
		const store = new Map<string, string>();
		(globalThis as any).localStorage = {
			getItem: (k: string) => store.get(k) ?? null,
			setItem: (k: string, v: string) => void store.set(k, v),
		};
		writeLastPage(5, 42);
		expect(readLastPage(5)).toBe(42);
		expect(readLastPage(6)).toBeNull();
		(globalThis as any).localStorage = {
			getItem: () => {
				throw new Error('blocked');
			},
			setItem: () => {
				throw new Error('blocked');
			},
		};
		expect(readLastPage(5)).toBeNull();
		expect(() => writeLastPage(5, 1)).not.toThrow();
		delete (globalThis as any).localStorage;
	});
});

describe('appendToLoadedPage', () => {
	const deps = () => ({
		uploadPageFile: jest.fn(async () => ({ id: 9 })),
		appendToPage: jest.fn(async () => ({ id: 4, slug: 'doc', title: 'Doc', version: 3 })),
		relayPageAppended: jest.fn(async () => undefined),
	});

	it('sends the workspace the page list came from with the upload, the append and the relay', async () => {
		const d = deps();
		const file = new File(['x'], 'shot.png');
		await appendToLoadedPage({ id: -3, code: 'local-a' }, 4, { text: 'note', section: 'Log', screenshot: file }, d);
		expect(d.uploadPageFile).toHaveBeenCalledWith(4, file, -3);
		expect(d.appendToPage).toHaveBeenCalledWith(4, {
			markdown: '![](tmgr://file/9)\n\nnote',
			heading: 'Log',
			workspace_id: -3,
		});
		expect(d.relayPageAppended).toHaveBeenCalledWith({
			workspace_code: 'local-a',
			page: { id: 4, slug: 'doc', title: 'Doc', version: 3 },
		});
	});

	it('makes the append fail closed when the active workspace is no longer the loaded one', async () => {
		const d = deps();
		await appendToLoadedPage({ id: -3, code: 'local-a' }, 4, { text: 'note', section: '', screenshot: null }, d);
		const body = (d.appendToPage.mock.calls[0] as any[])[1];
		expect(crossesWorkspaces('local', body, undefined, -3)).toBe(false);
		expect(crossesWorkspaces('local', body, undefined, -9)).toBe(true);
		expect(crossesWorkspaces('server', body, undefined, null)).toBe(true);
	});

	it('appends text only without an upload', async () => {
		const d = deps();
		await appendToLoadedPage({ id: 7, code: 'w' }, 4, { text: 'note', section: '', screenshot: null }, d);
		expect(d.uploadPageFile).not.toHaveBeenCalled();
		expect(d.appendToPage).toHaveBeenCalledWith(4, { markdown: 'note', heading: undefined, workspace_id: 7 });
	});
});
