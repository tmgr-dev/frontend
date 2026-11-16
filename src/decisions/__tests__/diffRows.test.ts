import { buildDiffRows, keySignature, withPinnedIndexes } from '../diffRows';
import type { DiffGroup, DiffLine } from '../types';

const lines = (n: number): DiffLine[] =>
	Array.from({ length: n }, (_, i) => ({
		kind: 'ctx' as const,
		old: i + 1,
		new: i + 1,
		text: `line ${i + 1}`,
	}));

const group = (
	over: Partial<DiffGroup['view']> = {},
	label = 'r',
): DiffGroup => ({
	key: label,
	label,
	kind: 'repo',
	view: {
		truncated: false,
		files: [
			{
				path: 'a.rs',
				added: 0,
				removed: 0,
				hunks: [
					{ header: '@@ -1,3 +1,3 @@', lines: lines(3) },
					{ header: '@@ -10,2 +10,2 @@', lines: lines(2) },
				],
			},
			{ path: 'b.rs', added: 0, removed: 0, hunks: [] },
		],
		...over,
	},
});

describe('buildDiffRows', () => {
	it('flattens files, hunks and lines into one row list', () => {
		const { rows } = buildDiffRows([group()], new Set());
		expect(rows.map((r) => r.kind)).toEqual([
			'file',
			'hunk',
			'line',
			'line',
			'line',
			'hunk',
			'line',
			'line',
			'file',
		]);
	});

	it('marks the last row of every file as its bottom edge', () => {
		const { rows } = buildDiffRows([group()], new Set());
		const bottoms = rows.flatMap((r, i) =>
			'bottom' in r && r.bottom ? [i] : [],
		);
		expect(bottoms).toEqual([7, 8]);
	});

	it('adds group headers and notes only where they apply', () => {
		const single = buildDiffRows([group({ truncated: true })], new Set());
		expect(single.rows[0].kind).toBe('note');
		const multi = buildDiffRows(
			[group({}, 'one'), group({ files: [], error: 'boom' }, 'two')],
			new Set(),
		);
		const kinds = multi.rows.map((r) => r.kind);
		expect(kinds.filter((k) => k === 'group')).toHaveLength(2);
		expect(multi.rows[multi.rows.length - 1]).toMatchObject({
			kind: 'note',
			note: 'error',
			text: 'boom',
		});
	});

	it('inserts a thread row right after the anchored line', () => {
		const { rows, threadIndex, lineIndex } = buildDiffRows(
			[group()],
			new Set(['a.rs:2:new']),
		);
		const at = threadIndex.get('a.rs:2:new') as number;
		expect(lineIndex.get('a.rs:2:new')).toBe(at - 1);
		expect(rows[at]).toMatchObject({ kind: 'thread', key: 'a.rs:2:new' });
		expect(rows[at - 1]).toMatchObject({ kind: 'line', key: 'a.rs:2:new' });
		expect(rows.filter((r) => r.kind === 'thread')).toHaveLength(1);
	});

	it('anchors a repeated key only once, on its first line', () => {
		const dup = group();
		dup.view.files[0].hunks[1].lines = lines(3);
		const { rows } = buildDiffRows([dup], new Set(['a.rs:1:new']));
		expect(rows.filter((r) => r.kind === 'thread')).toHaveLength(1);
	});

	it('qualifies the thread key with the group label when there are several', () => {
		const { threadIndex } = buildDiffRows(
			[group({}, 'one'), group({}, 'two')],
			new Set(['two:a.rs:1:new']),
		);
		expect(threadIndex.has('two:a.rs:1:new')).toBe(true);
	});

	it('flattens 20k lines quickly and tracks the widest line', () => {
		const big = group();
		big.view.files[0].hunks = [
			{
				header: '@@ -1 +1 @@',
				lines: [
					...lines(20000),
					{ kind: 'add', new: 20001, text: 'x'.repeat(300) },
				],
			},
		];
		const started = Date.now();
		const { rows, maxChars } = buildDiffRows([big], new Set());
		expect(Date.now() - started).toBeLessThan(500);
		expect(rows).toHaveLength(20004);
		expect(maxChars).toBe(301);
	});
});

describe('withPinnedIndexes', () => {
	it('merges, sorts, dedupes and drops out-of-range indexes', () => {
		expect(withPinnedIndexes([5, 6, 7], [100, 6, 2, 999, -1], 500)).toEqual([
			2, 5, 6, 7, 100,
		]);
	});
});

describe('keySignature', () => {
	it('is order and duplicate independent', () => {
		expect(keySignature(['b', 'a', 'b'])).toBe(keySignature(['a', 'b']));
		expect(keySignature([])).toBe('');
	});
});
