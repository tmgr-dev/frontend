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

	it('drops only truly empty list blocks (D-002)', () => {
		expect(
			normalizeBlocks(
				doc(
					{ type: 'list', data: { style: 'unordered', items: [] } },
					{ type: 'list', data: {} },
					{ type: 'list', data: { items: null } },
					{ type: 'checklist', data: { items: [] } },
					{ type: 'checklist', data: { items: [null, undefined] } },
					{ type: 'checklist', data: { items: [null, 5] } },
					{ type: 'checklist' },
					{ type: 'paragraph', data: { text: 'kept' } },
				),
			).blocks,
		).toEqual([
			{
				type: 'list',
				data: {
					style: 'checklist',
					meta: {},
					items: [item('5', { checked: false })],
				},
			},
			{ type: 'paragraph', data: { text: 'kept' } },
		]);
	});

	it('keeps readable text instead of blanking it', () => {
		const [list, checklist] = normalizeBlocks(
			doc(
				{
					type: 'list',
					data: {
						style: 'unordered',
						items: [
							{ content: 17 },
							{ content: 17, text: 'recoverable text' },
							{ text: 'text only' },
							{ text: 2.5, items: [{ content: 'child' }] },
							5,
							null,
							'plain',
						],
					},
				},
				{ type: 'checklist', data: { items: [{ text: 0, checked: true }] } },
			),
		).blocks;
		expect(list.data.items).toEqual([
			item('17'),
			item('recoverable text'),
			item('text only'),
			item('2.5', {}, [item('child')]),
			item('5'),
			item('plain'),
		]);
		expect(checklist.data.items).toEqual([item('0', { checked: true })]);
	});

	it('prefers non-empty text over empty content', () => {
		expect(
			normalizeBlocks(
				doc({
					type: 'list',
					data: {
						items: [
							{ content: '', text: 'x' },
							{ content: '', text: 5 },
							{ content: '', text: { a: 1 } },
							{ content: '', text: '' },
						],
					},
				}),
			).blocks[0].data.items,
		).toEqual([item('x'), item('5'), item('{"a":1}'), item('')]);
	});

	it.each([
		['object content', [{ content: {} }], [item('{}')]],
		['escaped object content', [{ content: { a: '<b>&' } }], [item('{"a":"&lt;b&gt;&amp;"}')]],
		['boolean content with items', [{ content: true, items: [] }], [item('true')]],
		['NaN text', [{ text: NaN }], [item('NaN')]],
		['Infinity content', [{ content: Infinity }], [item('Infinity')]],
		['non-array nested items', [{ content: 'a', items: 'x' }], [item('a', {}, [item('x')])]],
		['object nested items', [{ content: 'a', items: { content: 'b' } }], [item('a', {}, [item('b')])]],
		['nested array item', [{ content: 'a', items: [[1]] }], [item('a', {}, [item('[1]')])]],
		['array item', ['a', ['b']], [item('a'), item('["b"]')]],
		['boolean item', [true, false], [item('true'), item('false')]],
	])('turns %s into text, siblings intact', (_, items, expected) => {
		const paragraph = { type: 'paragraph', data: { text: 'p' } };
		const sibling = { type: 'list', data: { items: ['s'] } };
		const out = normalizeBlocks(
			doc(sibling, { type: 'list', data: { items } }, paragraph),
		);
		expect(out.blocks[1].data.items).toEqual(expected);
		expect(out.blocks[0].data.items).toEqual([item('s')]);
		expect(out.blocks[2]).toBe(paragraph);
		expect(JSON.stringify(normalizeBlocks(out))).toBe(JSON.stringify(out));
	});

	it.each([
		['string', 'abc'],
		['object', { 0: 'a' }],
		['number', 5],
	])('keeps a list with %s data.items untouched', (_, items) => {
		const paragraph = { type: 'paragraph', data: { text: 'p' } };
		for (const type of ['list', 'checklist']) {
			const raw = { id: 'r', type, data: { items } };
			const sibling = { type: 'list', data: { items: ['s'] } };
			const out = normalizeBlocks(doc(sibling, raw, paragraph)).blocks;
			expect(out[1]).toBe(raw);
			expect(out[0]).toEqual({
				type: 'list',
				data: { style: 'unordered', meta: {}, items: [item('s')] },
			});
			expect(out[2]).toBe(paragraph);
			expect(normalizeBlocks({ blocks: out }).blocks[1]).toBe(raw);
		}
	});

	it.each([
		['string', 'x'],
		['array', [{ content: 'x' }]],
	])('keeps a list with %s data untouched', (_, data) => {
		for (const type of ['list', 'checklist']) {
			const raw = { type, data };
			expect(normalizeBlocks(doc(raw)).blocks[0]).toBe(raw);
		}
	});

	it.each(['', [], 0, true])('drops a list with empty data %p', (data) => {
		expect(normalizeBlocks(doc({ type: 'list', data })).blocks).toEqual([]);
	});

	it('keeps unknown fields of items without content or text as text', () => {
		const [block] = normalizeBlocks(
			doc({
				type: 'list',
				data: {
					items: [
						{ content: 'a', items: { 0: { content: 'x' }, 2: { content: 'y' } } },
						{ label: 'Buy milk', checked: true, id: null },
					],
				},
			}),
		).blocks;
		expect(block.data.items).toEqual([
			item('a', {}, [
				item('{"0":{"content":"x"},"2":{"content":"y"}}'),
			]),
			item('{"label":"Buy milk"}'),
		]);
		expect(normalizeBlocks({ blocks: [block] }).blocks[0]).toEqual(block);
	});

	it('normalises a mixed doc without mutating it, in list@2 key order, idempotently (QA TM-591)', () => {
		const freeze = (value: any): any => {
			if (value && typeof value === 'object') {
				Object.values(value).forEach(freeze);
				Object.freeze(value);
			}
			return value;
		};
		const raw = { type: 'list', data: { items: { content: 'a', items: 'x' } } };
		const input = freeze(
			doc(
				{ type: 'list', data: { style: 'ordered', items: ['a', 3] } },
				{ type: 'checklist', data: { items: [5, { text: 't', checked: true, items: [{ content: 7 }] }] } },
				raw,
			),
		);
		const copy = structuredClone(input);
		const out = normalizeBlocks(input);
		expect(input).toEqual(copy);
		expect(out.blocks[2]).toBe(raw);
		const keys = (items: any[]): string[][] =>
			items.flatMap((i) => [Object.keys(i), ...keys(i.items)]);
		const built = [...out.blocks[0].data.items, ...out.blocks[1].data.items];
		expect(new Set(keys(built).map((k) => k.join()))).toEqual(new Set(['content,meta,items']));
		expect(Object.keys(out.blocks[1].data)).toEqual(['style', 'meta', 'items']);
		expect(out.blocks[1].data.items).toEqual([
			item('5', { checked: false }),
			item('t', { checked: true }, [item('7', { checked: false })]),
		]);
		expect(JSON.stringify(normalizeBlocks(out))).toBe(JSON.stringify(out));
		expect(JSON.stringify(normalizeBlocks(JSON.stringify(input)))).toBe(JSON.stringify(out));
	});

	it('defaults a missing style to unordered', () => {
		expect(
			normalizeBlocks(doc({ type: 'list', data: { items: ['a'] } })).blocks[0]
				.data,
		).toEqual({ style: 'unordered', meta: {}, items: [item('a')] });
	});
});
