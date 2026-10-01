import type { PageSummary } from '@/actions/tmgr/pages';
import {
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
