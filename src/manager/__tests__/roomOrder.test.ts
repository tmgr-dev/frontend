import {
	arrangeByKeys,
	loadSortMode,
	saveSortMode,
	sortRooms,
	type OrderStorage,
} from '../roomOrder';
import { roomKey, type RoomInfo } from '../types';

const room = (session: string, ts?: number): RoomInfo => ({
	room: { kind: 'session', machine: 'mac', session },
	title: session,
	last: ts
		? {
				v: 1,
				id: `m${session}`,
				room: { kind: 'session', machine: 'mac', session },
				author: { kind: 'owner', id: 'o' },
				ts_ms: ts,
				kind: 'text',
				body_md: 'x',
			}
		: undefined,
});

const names = (rooms: RoomInfo[]) => rooms.map((r) => r.title);

describe('sortRooms', () => {
	const rooms = [room('a', 10), room('b'), room('c', 30), room('d', 20)];

	it('keeps the given order in default mode', () => {
		expect(names(sortRooms(rooms, 'default'))).toEqual(['a', 'b', 'c', 'd']);
	});

	it('puts the newest last message first and rooms without messages last', () => {
		expect(names(sortRooms(rooms, 'recent'))).toEqual(['c', 'd', 'a', 'b']);
	});

	it('does not mutate its input', () => {
		sortRooms(rooms, 'recent');
		expect(names(rooms)).toEqual(['a', 'b', 'c', 'd']);
	});

	it('follows a new message in a room', () => {
		const next = [room('a', 99), ...rooms.slice(1)];
		expect(names(sortRooms(next, 'recent'))[0]).toBe('a');
	});
});

describe('arrangeByKeys', () => {
	it('keeps a frozen order, appends new rooms and drops gone ones', () => {
		const rooms = [room('a'), room('b'), room('n')];
		const keys = ['x', 'b', 'a'].map((s) =>
			roomKey({ kind: 'session', machine: 'mac', session: s }),
		);
		expect(names(arrangeByKeys(rooms, keys))).toEqual(['b', 'a', 'n']);
	});
});

describe('stored sort mode', () => {
	const memory = (): OrderStorage & { data: Record<string, string> } => {
		const data: Record<string, string> = {};
		return {
			data,
			getItem: (k) => data[k] ?? null,
			setItem: (k, v) => {
				data[k] = v;
			},
		};
	};

	it('round-trips and defaults', () => {
		const s = memory();
		expect(loadSortMode(s)).toBe('default');
		saveSortMode('recent', s);
		expect(loadSortMode(s)).toBe('recent');
		saveSortMode('default', s);
		expect(loadSortMode(s)).toBe('default');
	});

	it('survives broken or missing storage', () => {
		const broken: OrderStorage = {
			getItem: () => {
				throw new Error('x');
			},
			setItem: () => {
				throw new Error('x');
			},
		};
		expect(loadSortMode(broken)).toBe('default');
		expect(() => saveSortMode('recent', broken)).not.toThrow();
		expect(loadSortMode(null)).toBe('default');
		expect(() => saveSortMode('recent', null)).not.toThrow();
	});
});
