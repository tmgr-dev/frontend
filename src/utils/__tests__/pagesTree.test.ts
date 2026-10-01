import type { PageSummary } from '@/actions/tmgr/pages';
import {
	ancestorIds,
	applyMove,
	buildPagesTree,
	descendantIds,
	loadExpanded,
	pagesAvailable,
	saveExpanded,
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

describe('descendantIds / ancestorIds', () => {
	const pages = [
		page(1),
		page(2, { parent_id: 1 }),
		page(3, { parent_id: 2 }),
		page(4),
	];
	it('collects the whole subtree', () => {
		expect(Array.from(descendantIds(pages, 1)).sort()).toEqual([1, 2, 3]);
	});
	it('lists ancestors nearest first', () => {
		expect(ancestorIds(pages, 3)).toEqual([2, 1]);
		expect(ancestorIds(pages, 4)).toEqual([]);
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
	it('is disabled for local workspaces even when the toggle is on', () => {
		expect(pagesAvailable({ id: 1, is_local: true }, true)).toBe(false);
		expect(pagesAvailable({ id: -3 }, true)).toBe(false);
	});
	it('is disabled without a workspace', () => {
		expect(pagesAvailable(null, true)).toBe(false);
	});
});
