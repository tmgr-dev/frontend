import type { PageSummary } from '@/actions/tmgr/pages';
import {
	RECENT_PAGES_LIMIT,
	ancestorIds,
	applyMove,
	buildPagesTree,
	descendantIds,
	loadExpanded,
	pagesAvailable,
	pagesWorkspaceId,
	saveExpanded,
	sidebarPages,
	visibleRootCount,
} from '../pagesTree';

const page = (id: number, extra: Partial<PageSummary> = {}): PageSummary => ({
	id,
	title: `p${id}`,
	slug: `p${id}`,
	type: 'plain',
	parent_id: null,
	position: 0,
	pinned: false,
	updated_at: '2026-10-01T00:00:00Z',
	...extra,
});

describe('buildPagesTree', () => {
	it('puts pinned roots first, then orders by position', () => {
		const tree = buildPagesTree([
			page(1, { position: 2 }),
			page(2, { position: 1 }),
			page(3, { position: 5, pinned: true }),
		]);
		expect(tree.map((n) => n.id)).toEqual([3, 2, 1]);
	});

	it('nests children and sorts each level', () => {
		const tree = buildPagesTree([
			page(1),
			page(2, { parent_id: 1, position: 1 }),
			page(3, { parent_id: 1, position: 0 }),
			page(4, { parent_id: 3 }),
		]);
		expect(tree).toHaveLength(1);
		expect(tree[0].children.map((n) => n.id)).toEqual([3, 2]);
		expect(tree[0].children[0].children.map((n) => n.id)).toEqual([4]);
	});

	it('treats a page with a missing parent as a root', () => {
		const tree = buildPagesTree([page(1), page(2, { parent_id: 99 })]);
		expect(tree.map((n) => n.id).sort()).toEqual([1, 2]);
	});

	it('does not mutate the input', () => {
		const input = [page(1), page(2, { parent_id: 1 })];
		buildPagesTree(input);
		expect(input).toEqual([page(1), page(2, { parent_id: 1 })]);
	});
});

describe('applyMove', () => {
	const base = [
		page(1, { position: 0 }),
		page(2, { position: 1 }),
		page(3, { position: 2 }),
		page(4, { parent_id: 1, position: 0 }),
	];

	it('reorders among siblings and renumbers positions', () => {
		const moved = applyMove(base, 3, null, 0);
		const roots = buildPagesTree(moved).map((n) => [n.id, n.position]);
		expect(roots).toEqual([
			[3, 0],
			[1, 1],
			[2, 2],
		]);
	});

	it('re-parents a page at a position', () => {
		const moved = applyMove(base, 2, 1, 0);
		const tree = buildPagesTree(moved);
		expect(tree.find((n) => n.id === 1)!.children.map((n) => n.id)).toEqual([
			2, 4,
		]);
	});

	it('clamps an out-of-range position and ignores an unknown id', () => {
		expect(applyMove(base, 1, null, 99).find((p) => p.id === 1)!.position).toBe(
			2,
		);
		expect(applyMove(base, 77, null, 0)).toBe(base);
	});
});

describe('descendantIds', () => {
	const pages = [
		page(1),
		page(2, { parent_id: 1 }),
		page(3, { parent_id: 2 }),
		page(4),
	];
	it('collects the whole subtree', () => {
		expect(Array.from(descendantIds(pages, 1)).sort()).toEqual([1, 2, 3]);
	});
});

describe('sidebarPages', () => {
	const at = (minute: number) =>
		`2026-10-01T10:${String(minute).padStart(2, '0')}:00Z`;

	it('lists pinned pages in tree order, then the newest edits flat', () => {
		const pages = [
			page(1, { updated_at: at(1), position: 1 }),
			page(2, { parent_id: 1, pinned: true, updated_at: at(9) }),
			page(3, { pinned: true, position: 2, updated_at: at(2) }),
			page(4, { parent_id: 1, updated_at: at(8) }),
			page(5, { updated_at: at(5) }),
			page(6, { updated_at: at(5) }),
		];
		const { pinned, recent } = sidebarPages(pages);
		expect(pinned.map((p) => p.id)).toEqual([3, 2]);
		expect(recent.map((p) => p.id)).toEqual([4, 6, 5, 1]);
		expect(RECENT_PAGES_LIMIT).toBe(5);
		expect([...pinned, ...recent].every((p) => !p.children.length)).toBe(true);
	});

	it('caps the recent list and never repeats pinned pages', () => {
		const pages = Array.from({ length: 15 }, (_, i) =>
			page(i + 1, { updated_at: at(i + 1), pinned: i + 1 === 15 }),
		);
		const { pinned, recent } = sidebarPages(pages);
		expect(pinned.map((p) => p.id)).toEqual([15]);
		expect(recent.map((p) => p.id)).toEqual([14, 13, 12, 11, 10]);
		expect(sidebarPages(pages, 3).recent).toHaveLength(3);
		expect(sidebarPages([])).toEqual({ pinned: [], recent: [] });
	});
});

describe('ancestorIds', () => {
	const tree = [
		page(-1, {}),
		page(-2, { parent_id: -1, slug: 'b' }),
		page(-3, { parent_id: -2, slug: 'c' }),
		page(-4, { slug: 'd' }),
	];

	it('lists every ancestor of a page by slug, including negative ids', () => {
		expect(ancestorIds(tree, 'c').sort((a, b) => a - b)).toEqual([-2, -1]);
		expect(ancestorIds(tree, 'b')).toEqual([-1]);
	});

	it('is empty for roots, unknown slugs and parent cycles', () => {
		expect(ancestorIds(tree, 'd')).toEqual([]);
		expect(ancestorIds(tree, 'nope')).toEqual([]);
		const loop = [
			page(1, { parent_id: 2, slug: 'x' }),
			page(2, { parent_id: 1, slug: 'y' }),
		];
		expect(ancestorIds(loop, 'x').sort()).toEqual([1, 2]);
	});
});

describe('visibleRootCount', () => {
	it('grows to include the root of the active page', () => {
		expect(visibleRootCount(500, 100, 3)).toBe(100);
		expect(visibleRootCount(500, 100, 250)).toBe(251);
		expect(visibleRootCount(50, 100, -1)).toBe(50);
	});
});

describe('expanded state persistence', () => {
	const store: Record<string, string> = {};
	beforeEach(() => {
		Object.keys(store).forEach((k) => delete store[k]);
		(global as any).localStorage = {
			getItem: (k: string) => (k in store ? store[k] : null),
			setItem: (k: string, v: string) => {
				store[k] = v;
			},
		};
	});
	afterEach(() => {
		delete (global as any).localStorage;
	});

	it('round-trips per workspace', () => {
		saveExpanded(5, new Set([1, 2]));
		saveExpanded(6, new Set([9]));
		expect(Array.from(loadExpanded(5))).toEqual([1, 2]);
		expect(Array.from(loadExpanded(6))).toEqual([9]);
	});

	it('survives corrupt data and throwing storage', () => {
		store['pages-expanded-5'] = '{oops';
		expect(loadExpanded(5).size).toBe(0);
		(global as any).localStorage = {
			getItem: () => {
				throw new Error('blocked');
			},
			setItem: () => {
				throw new Error('blocked');
			},
		};
		expect(loadExpanded(5).size).toBe(0);
		expect(() => saveExpanded(5, new Set([1]))).not.toThrow();
	});
});

describe('pagesAvailable', () => {
	it('requires the toggle', () => {
		expect(pagesAvailable({ id: 1 }, false)).toBe(false);
		expect(pagesAvailable({ id: 1 }, true)).toBe(true);
	});
	it('follows the toggle for local workspaces too', () => {
		expect(pagesAvailable({ id: 1, is_local: true }, true)).toBe(true);
		expect(pagesAvailable({ id: -3 }, true)).toBe(true);
		expect(pagesAvailable({ id: -3 }, false)).toBe(false);
	});
	it('is disabled without a workspace', () => {
		expect(pagesAvailable(null, true)).toBe(false);
	});
});

describe('pagesWorkspaceId', () => {
	it('keeps the negative id of a local workspace', () => {
		expect(pagesWorkspaceId(-48213)).toBe(-48213);
		expect(pagesWorkspaceId('-48213')).toBe(-48213);
	});

	it('keeps a cloud id', () => {
		expect(pagesWorkspaceId('56')).toBe(56);
	});

	it('is null without a workspace', () => {
		expect(pagesWorkspaceId(null)).toBeNull();
		expect(pagesWorkspaceId(undefined)).toBeNull();
		expect(pagesWorkspaceId(0)).toBeNull();
		expect(pagesWorkspaceId('abc')).toBeNull();
	});
});
