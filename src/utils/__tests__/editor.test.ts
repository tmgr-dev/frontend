import { normalizeBlocks } from '../editor';

const doc = (...blocks: unknown[]) => ({ time: 1, blocks, version: '2.31.0' });
const item = (content: string, meta = {}, items: unknown[] = []) => ({
	content,
	meta,
	items,
});

describe('normalizeBlocks', () => {
	it.each([null, undefined, '', 'not json', 42, {}, { blocks: null }])(
		'returns an empty doc for %p',
		(value) => {
			expect(normalizeBlocks(value).blocks).toEqual([]);
		},
	);

	it('parses JSON strings and keeps doc fields', () => {
		const input = doc({ id: 'p', type: 'paragraph', data: { text: 'a' } });
		expect(normalizeBlocks(JSON.stringify(input))).toEqual(input);
	});

	it('converts old string lists', () => {
		expect(
			normalizeBlocks(
				doc({
					id: 'l',
					type: 'list',
					data: { style: 'ordered', items: ['a', 'b'] },
				}),
			).blocks,
		).toEqual([
			{
				id: 'l',
				type: 'list',
				data: {
					style: 'ordered',
					meta: { counterType: 'numeric' },
					items: [item('a'), item('b')],
				},
			},
		]);
	});

	it('adds meta to old nested items recursively', () => {
		const [block] = normalizeBlocks(
			doc({
				type: 'list',
				data: {
					style: 'unordered',
					items: [
						{
							content: 'a',
							items: [{ content: 'a1', items: [{ content: 'a2' }] }],
						},
					],
				},
			}),
		).blocks;
		expect(block.data).toEqual({
			style: 'unordered',
			meta: {},
			items: [item('a', {}, [item('a1', {}, [item('a2')])])],
		});
	});

	it('converts old checklist blocks', () => {
		const [block] = normalizeBlocks(
			doc({
				id: 'c',
				type: 'checklist',
				data: {
					items: [
						{ text: 'done', checked: true },
						{ text: 'todo', checked: false },
						{ text: 'odd', checked: 'yes' },
						{ checked: true },
					],
				},
			}),
		).blocks;
		expect(block).toEqual({
			id: 'c',
			type: 'list',
			data: {
				style: 'checklist',
				meta: {},
				items: [
					item('done', { checked: true }),
					item('todo', { checked: false }),
					item('odd', { checked: false }),
					item('', { checked: true }),
				],
			},
		});
	});

	it('leaves new-format data unchanged and is idempotent', () => {
		const input = doc(
			{
				id: 'o',
				type: 'list',
				data: {
					style: 'ordered',
					meta: { start: 3, counterType: 'lower-roman' },
					items: [item('a', {}, [item('a1')])],
				},
			},
			{
				id: 'c',
				type: 'list',
				data: {
					style: 'checklist',
					meta: {},
					items: [
						item('x', { checked: true }, [item('y', { checked: false })]),
					],
				},
			},
		);
		const once = normalizeBlocks(input);
		expect(once).toEqual(input);
		expect(JSON.stringify(once)).toBe(JSON.stringify(input));
		expect(normalizeBlocks(once)).toEqual(once);
		const legacy = doc(
			{ type: 'list', data: { style: 'unordered', items: ['a'] } },
			{ type: 'checklist', data: { items: [{ text: 't', checked: true }] } },
		);
		expect(normalizeBlocks(normalizeBlocks(legacy))).toEqual(
			normalizeBlocks(legacy),
		);
	});

	it('keeps unknown blocks and does not mutate the input', () => {
		const input = doc(
			{ id: 'w', type: 'warning', data: { title: 't', message: 'm' } },
			{
				id: 'c',
				type: 'checklist',
				data: { items: [{ text: 'a', checked: true }] },
			},
		);
		const copy = structuredClone(input);
		const out = normalizeBlocks(input);
		expect(input).toEqual(copy);
		expect(out).not.toBe(input);
		expect(out.blocks[0]).toBe(input.blocks[0]);
	});

	it('drops list blocks without usable items (D-002)', () => {
		expect(
			normalizeBlocks(
				doc(
					{ type: 'list', data: { style: 'unordered', items: [] } },
					{ type: 'list', data: {} },
					{ type: 'checklist', data: { items: [] } },
					{ type: 'checklist', data: { items: [null, 5] } },
					{ type: 'checklist' },
					{ type: 'paragraph', data: { text: 'kept' } },
				),
			).blocks,
		).toEqual([{ type: 'paragraph', data: { text: 'kept' } }]);
	});

	it('defaults a missing style to unordered', () => {
		expect(
			normalizeBlocks(doc({ type: 'list', data: { items: ['a'] } })).blocks[0]
				.data,
		).toEqual({ style: 'unordered', meta: {}, items: [item('a')] });
	});
});
