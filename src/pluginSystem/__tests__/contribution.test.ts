import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import {
	isEmptyContribution,
	MAX_CONTRIBUTION_BYTES,
	reduceContribution,
} from '../contribution';

const FIXTURES = join(__dirname, '..', '__fixtures__', 'contribution');
const load = (dir: string) =>
	readdirSync(join(FIXTURES, dir))
		.filter((name) => name.endsWith('.json'))
		.map((name) => [
			name,
			JSON.parse(readFileSync(join(FIXTURES, dir, name), 'utf8')),
		]);

const TONES = ['default', 'muted', 'success', 'warning', 'danger'];
const ID = /^[a-z0-9_-]{1,40}$/;
const CONTROL = /[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/;
const obj = (v: any) => !!v && typeof v === 'object' && !Array.isArray(v);
const only = (value: any, keys: string[]) =>
	Object.keys(value).every((key) => keys.includes(key));
const str = (v: any, max: number, breaks = false) =>
	typeof v === 'string' &&
	v.length >= 1 &&
	v.length <= max &&
	(breaks || !/[\n\t]/.test(v)) &&
	!CONTROL.test(v);
const tone = (v: any) => v === undefined || TONES.includes(v);

const isValidContribution = (doc: any): boolean => {
	if (!obj(doc) || !only(doc, ['v', 'name', 'badges', 'sections']))
		return false;
	if (doc.v !== undefined && doc.v !== 1) return false;
	if (doc.name !== undefined && !str(doc.name, 60)) return false;
	const badges = doc.badges ?? [];
	const sections = doc.sections ?? [];
	if (!Array.isArray(badges) || badges.length > 3) return false;
	if (!Array.isArray(sections) || sections.length > 4) return false;
	if (new Set(badges.map((b: any) => b?.id)).size !== badges.length)
		return false;
	if (new Set(sections.map((s: any) => s?.id)).size !== sections.length)
		return false;
	if (
		!badges.every(
			(b: any) =>
				obj(b) &&
				only(b, ['id', 'text', 'tone']) &&
				ID.test(b.id) &&
				str(b.text, 24) &&
				tone(b.tone),
		)
	)
		return false;
	if (
		new TextEncoder().encode(JSON.stringify(doc)).length >
		MAX_CONTRIBUTION_BYTES
	)
		return false;
	return sections.every((s: any) => {
		if (!obj(s) || !only(s, ['id', 'title', 'tree'])) return false;
		if (!ID.test(s.id) || !str(s.title, 60)) return false;
		let nodes = 0;
		const node = (n: any, depth: number): boolean => {
			if (depth > 6 || ++nodes > 80 || !obj(n)) return false;
			switch (n.type) {
				case 'stack':
					return (
						only(n, ['type', 'direction', 'children']) &&
						(n.direction === undefined ||
							['column', 'row'].includes(n.direction)) &&
						Array.isArray(n.children) &&
						n.children.length <= 40 &&
						n.children.every((c: any) => node(c, depth + 1))
					);
				case 'heading':
					return (
						only(n, ['type', 'text', 'level']) &&
						str(n.text, 120) &&
						(n.level === undefined || [1, 2, 3].includes(n.level))
					);
				case 'text':
					return (
						only(n, ['type', 'text', 'tone']) &&
						str(n.text, 2000, true) &&
						tone(n.tone)
					);
				case 'divider':
					return only(n, ['type']);
				case 'keyValue':
					return (
						only(n, ['type', 'items']) &&
						Array.isArray(n.items) &&
						n.items.length <= 20 &&
						n.items.every(
							(i: any) =>
								obj(i) &&
								only(i, ['key', 'value']) &&
								str(i.key, 60) &&
								str(i.value, 200),
						)
					);
				case 'badge':
					return (
						only(n, ['type', 'text', 'tone']) && str(n.text, 24) && tone(n.tone)
					);
				case 'progress':
					return (
						only(n, ['type', 'value', 'label', 'tone']) &&
						typeof n.value === 'number' &&
						n.value >= 0 &&
						n.value <= 10 &&
						(n.label === undefined || str(n.label, 60)) &&
						tone(n.tone)
					);
				case 'stat':
					return (
						only(n, ['type', 'label', 'value', 'tone']) &&
						str(n.label, 60) &&
						str(n.value, 40) &&
						tone(n.tone)
					);
				case 'list':
					return (
						only(n, ['type', 'items']) &&
						Array.isArray(n.items) &&
						n.items.length <= 50 &&
						n.items.every((c: any) => node(c, depth + 1))
					);
				case 'markdown':
					return only(n, ['type', 'text']) && str(n.text, 4000, true);
				case 'taskLink':
					return (
						only(n, ['type', 'taskId', 'text']) &&
						Number.isSafeInteger(n.taskId) &&
						n.taskId > 0 &&
						str(n.text, 120)
					);
				default:
					return false;
			}
		};
		return node(s.tree, 1);
	});
};

const normalise = (doc: any) => {
	const { v, ...rest } = doc;
	for (const key of ['badges', 'sections'])
		if (Array.isArray(rest[key]) && rest[key].length === 0) delete rest[key];
	return rest;
};

describe('shared fixtures', () => {
	it.each(load('valid'))('keeps valid/%s unchanged', (_name, doc) => {
		expect(isValidContribution(doc)).toBe(true);
		expect(normalise(reduceContribution(doc))).toEqual(normalise(doc));
	});

	it.each(load('invalid'))(
		'never turns invalid/%s into a violation',
		(_name, doc) => {
			expect(isValidContribution(doc)).toBe(false);
			expect(isValidContribution(reduceContribution(doc))).toBe(true);
		},
	);
});

describe('reduceContribution', () => {
	it('maps desktop badge colors to tones and prefers an explicit tone', () => {
		const colors = [
			'gray',
			'green',
			'yellow',
			'orange',
			'red',
			'blue',
			'purple',
		];
		const doc = reduceContribution({
			badges: [
				{ id: 'a', text: 'x', color: 'gray' },
				{ id: 'b', text: 'x', color: 'red', tone: 'success' },
				{ id: 'c', text: 'x', color: 'nope' },
			],
		});
		expect(doc.badges).toEqual([
			{ id: 'a', text: 'x', tone: 'muted' },
			{ id: 'b', text: 'x', tone: 'success' },
			{ id: 'c', text: 'x' },
		]);
		const tones = colors.map(
			(color) =>
				reduceContribution({ badges: [{ id: 'a', text: 'x', color }] })
					.badges[0].tone,
		);
		expect(tones).toEqual([
			'muted',
			'success',
			'warning',
			'warning',
			'danger',
			'default',
			'default',
		]);
	});

	it('maps the desktop node vocabulary and drops the rest', () => {
		const doc = reduceContribution({
			sections: [
				{
					id: 's',
					title: 'T',
					tree: {
						type: 'stack',
						children: [
							'plain string',
							{ type: 'button', text: 'x' },
							{ type: 'badge', text: 'late', color: 'red', command: 'x' },
							{ type: 'progress', value: 0.5, color: 'green', label: 'L' },
							{
								type: 'keyValue',
								items: [
									{ key: 'a', value: 'b' },
									{ key: 'n', value: 3 },
									{ key: 'node', value: { type: 'text', text: 'tx' } },
									{
										key: 'stat',
										value: { type: 'stat', label: 'l', value: 'sv' },
									},
									{ key: 'bad', value: { type: 'stack', children: [] } },
								],
							},
							{ type: 'markdown', text: '**hi**' },
							{ type: 'taskLink', taskId: 4, text: 'Four', onClick: 'x' },
						],
					},
				},
			],
		});
		expect(doc.sections[0].tree).toEqual({
			type: 'stack',
			direction: 'column',
			children: [
				{ type: 'text', text: 'plain string' },
				{ type: 'badge', text: 'late', tone: 'danger' },
				{ type: 'progress', value: 0.5, label: 'L', tone: 'success' },
				{
					type: 'keyValue',
					items: [
						{ key: 'a', value: 'b' },
						{ key: 'n', value: '3' },
						{ key: 'node', value: 'tx' },
						{ key: 'stat', value: 'sv' },
					],
				},
				{ type: 'markdown', text: '**hi**' },
				{ type: 'taskLink', taskId: 4, text: 'Four' },
			],
		});
	});

	it('shortens strings, strips control characters and clamps counts', () => {
		const doc = reduceContribution({
			name: 'n'.repeat(100),
			badges: Array.from({ length: 5 }, (_, i) => ({
				id: `b${i}`,
				text: 'x'.repeat(40),
			})),
			sections: Array.from({ length: 6 }, (_, i) => ({
				id: `s${i}`,
				title: 'T',
				tree: { type: 'text', text: `a\u0007b\nc\td\r` },
			})),
		});
		expect(doc.name).toHaveLength(60);
		expect(doc.badges).toHaveLength(3);
		expect(doc.badges[0].text).toHaveLength(24);
		expect(doc.sections).toHaveLength(4);
		expect(doc.sections[0].tree).toEqual({ type: 'text', text: 'ab\nc\td' });
	});

	it('does not split a surrogate pair when cutting', () => {
		const doc = reduceContribution({
			badges: [{ id: 'a', text: 'x'.repeat(23) + '😀' }],
		});
		expect(doc.badges[0].text).toBe('x'.repeat(23));
	});

	it('drops deep trees and caps nodes per section', () => {
		let deep: any = { type: 'text', text: 'leaf' };
		for (let i = 0; i < 8; i++) deep = { type: 'stack', children: [deep] };
		expect(
			reduceContribution({ sections: [{ id: 's', title: 'T', tree: deep }] })
				.sections,
		).toEqual([]);
		const wide = {
			type: 'stack',
			children: Array.from({ length: 40 }, () => ({
				type: 'stack',
				children: [
					{ type: 'text', text: 'a' },
					{ type: 'text', text: 'b' },
				],
			})),
		};
		const doc = reduceContribution({
			sections: [{ id: 's', title: 'T', tree: wide }],
		});
		expect(isValidContribution(doc)).toBe(true);
		expect(doc.sections).toHaveLength(1);
	});

	it('drops sections from the end until the document fits', () => {
		const big = (id: string) => ({
			id,
			title: 'T',
			tree: {
				type: 'stack',
				children: Array.from({ length: 4 }, (_, i) => ({
					type: 'markdown',
					text: `${i}`.repeat(1000),
				})),
			},
		});
		const doc = reduceContribution({
			sections: [big('a'), big('b'), big('c')],
		});
		expect(doc.sections.map((s) => s.id)).toEqual(['a']);
		expect(
			new TextEncoder().encode(JSON.stringify(doc)).length,
		).toBeLessThanOrEqual(MAX_CONTRIBUTION_BYTES);
	});

	it('reports an empty result', () => {
		expect(isEmptyContribution(reduceContribution({}))).toBe(true);
		expect(isEmptyContribution(reduceContribution(null))).toBe(true);
		expect(
			isEmptyContribution(reduceContribution({ name: 'only a name' })),
		).toBe(true);
		expect(
			isEmptyContribution(
				reduceContribution({ badges: [{ id: 'a', text: 'x' }] }),
			),
		).toBe(false);
	});
});
