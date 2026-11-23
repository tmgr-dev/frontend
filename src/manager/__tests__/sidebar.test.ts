import { needsYou, rowTitle, stateCounts } from '../sidebar';
import type { RoomInfo } from '../types';

const mgr: RoomInfo = {
	room: { kind: 'manager', machine: 'm' },
	title: 'Manager',
	state: 'working',
};
const sess = (id: string, extra: Partial<RoomInfo> = {}): RoomInfo => ({
	room: { kind: 'session', machine: 'm', session: id },
	title: `Title ${id}`,
	agent: 'claude',
	...extra,
});

describe('stateCounts', () => {
	it('counts sessions by state and ignores the manager room', () => {
		const rooms = [
			mgr,
			sess('a', { state: 'working' }),
			sess('b', { state: 'working' }),
			sess('c', { state: 'waiting' }),
			sess('d', { state: 'failed' }),
			sess('e', { state: 'idle' }),
			sess('f'),
		];
		expect(stateCounts(rooms)).toEqual({ working: 2, waiting: 1, failed: 1 });
	});

	it('is all zero for no rooms', () => {
		expect(stateCounts([])).toEqual({ working: 0, waiting: 0, failed: 0 });
	});

	it('follows state changes', () => {
		const a = sess('a', { state: 'working' });
		expect(stateCounts([a]).working).toBe(1);
		expect(stateCounts([{ ...a, state: 'failed' }])).toEqual({
			working: 0,
			waiting: 0,
			failed: 1,
		});
	});
});

describe('needsYou', () => {
	it('lists permission, question and failed in that order', () => {
		const rows = needsYou([
			sess('f', { state: 'failed' }),
			sess('q', { state: 'asking' }),
			sess('p', { state: 'blocked' }),
			mgr,
		]);
		expect(rows.map((r) => [r.key, r.tag])).toEqual([
			['session:m:p', 'permission'],
			['session:m:q', 'question'],
			['session:m:f', 'failed'],
		]);
	});

	it('skips idle waiting, working and unknown states', () => {
		expect(
			needsYou([
				sess('w', { state: 'waiting' }),
				sess('x', { state: 'working' }),
				sess('s', { state: 'starting' }),
				sess('e', { state: 'exited' }),
				sess('n'),
			]),
		).toEqual([]);
	});
});

describe('stateCounts for sessions that need the owner', () => {
	it('counts blocked and asking sessions as waiting', () => {
		expect(
			stateCounts([
				sess('a', { state: 'blocked' }),
				sess('b', { state: 'asking' }),
				sess('c', { state: 'waiting' }),
			]).waiting,
		).toBe(3);
	});
});

describe('rowTitle', () => {
	it('uses agent and task id when both are present', () => {
		expect(rowTitle(sess('a', { task: 'TM-1021' }))).toBe('claude · TM-1021');
	});

	it('falls back to the room title', () => {
		expect(rowTitle(sess('a'))).toBe('Title a');
		expect(rowTitle(sess('a', { task: 'TM-1', agent: undefined }))).toBe(
			'Title a',
		);
	});
});
