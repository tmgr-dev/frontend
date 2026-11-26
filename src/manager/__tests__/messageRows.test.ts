import { buildRows } from '../messageRows';
import type { Author, ChatMessage } from '../types';

const labels = { today: 'Today', yesterday: 'Yesterday' };
const agent: Author = { kind: 'session', id: 'a1', agent: 'claude' };
const other: Author = { kind: 'session', id: 'a2', agent: 'codex' };
const owner: Author = { kind: 'owner', id: 'o' };
const MIN = 60_000;
const base = new Date(2026, 10, 26, 10, 0, 0).getTime();

const msg = (
	id: string,
	author: Author,
	ts: number,
	extra: Partial<ChatMessage> = {},
): ChatMessage => ({
	v: 1,
	id,
	room: { kind: 'manager', machine: 'mac' },
	author,
	ts_ms: ts,
	kind: 'text',
	body_md: id,
	...extra,
});

describe('buildRows', () => {
	it('groups consecutive messages of one author within five minutes', () => {
		const rows = buildRows(
			[
				msg('a', agent, base),
				msg('b', agent, base + MIN),
				msg('c', agent, base + 2 * MIN),
			],
			base,
			labels,
		);
		expect(rows.map((r) => r.grouped)).toEqual([false, true, true]);
	});

	it('breaks a run on author change', () => {
		const rows = buildRows(
			[msg('a', agent, base), msg('b', other, base + MIN)],
			base,
			labels,
		);
		expect(rows.map((r) => r.grouped)).toEqual([false, false]);
	});

	it('breaks a run when the gap is five minutes or more', () => {
		const rows = buildRows(
			[
				msg('a', agent, base),
				msg('b', agent, base + 5 * MIN - 1),
				msg('c', agent, base + 10 * MIN),
			],
			base,
			labels,
		);
		expect(rows.map((r) => r.grouped)).toEqual([false, true, false]);
	});

	it('breaks a run when an owner message sits between', () => {
		const rows = buildRows(
			[
				msg('a', agent, base),
				msg('b', owner, base + MIN),
				msg('c', agent, base + 2 * MIN),
			],
			base,
			labels,
		);
		expect(rows.map((r) => r.grouped)).toEqual([false, false, false]);
	});

	it('puts a day chip on the first message of each day only', () => {
		const day = 86_400_000;
		const rows = buildRows(
			[
				msg('a', agent, base - 3 * day),
				msg('b', agent, base - 3 * day + MIN),
				msg('c', agent, base - day),
				msg('d', agent, base),
				msg('e', agent, base + MIN),
			],
			base,
			labels,
		);
		expect(rows.map((r) => r.day !== undefined)).toEqual([
			true,
			false,
			true,
			true,
			false,
		]);
		expect(rows[2].day).toBe('Yesterday');
		expect(rows[3].day).toBe('Today');
	});

	it('does not group across midnight', () => {
		const late = new Date(2026, 10, 25, 23, 59, 0).getTime();
		const rows = buildRows(
			[msg('a', agent, late), msg('b', agent, late + 2 * MIN)],
			late + 3 * MIN,
			labels,
		);
		expect(rows[1].grouped).toBe(false);
		expect(rows[1].day).toBe('Today');
	});

	it('resolves reply_to against the full list', () => {
		const rows = buildRows(
			[msg('a', owner, base), msg('b', agent, base + MIN, { reply_to: 'a' })],
			base,
			labels,
		);
		expect(rows[1].replyTo?.id).toBe('a');
	});
});
