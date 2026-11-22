import { groupRoomsByMachine, sharedWithPhone } from '../machineTree';
import type { ManagerMachine, RoomInfo, TrustDevice } from '../types';

const mgr = (machine: string): RoomInfo => ({
	room: { kind: 'manager', machine },
	title: `Manager ${machine}`,
});
const sess = (machine: string, session: string): RoomInfo => ({
	room: { kind: 'session', machine, session },
	title: `${machine}/${session}`,
});

const top: ManagerMachine = {
	machine: 'mx_top0000000000aa',
	label: 'Studio',
	role: 'top',
	followers: ['mx_f1', 'mx_f2'],
	local: true,
};
const f1: ManagerMachine = { machine: 'mx_f1', label: 'Server', role: 'sub' };
const f2: ManagerMachine = { machine: 'mx_f2', label: null, role: 'sub' };

describe('groupRoomsByMachine', () => {
	it('returns null with a single machine', () => {
		const rooms = [mgr(top.machine), sess(top.machine, 's1')];
		expect(groupRoomsByMachine(rooms, [top])).toBeNull();
	});

	it('returns null when machines are unknown (older backend)', () => {
		const rooms = [mgr('a'), sess('b', 's1')];
		expect(groupRoomsByMachine(rooms, [])).toBeNull();
	});

	it('orders top first, then followers in machines order', () => {
		const rooms = [
			sess('mx_f2', 'x'),
			mgr('mx_f1'),
			sess(top.machine, 's1'),
			mgr('mx_f2'),
			mgr(top.machine),
			sess('mx_f1', 'y'),
		];
		const groups = groupRoomsByMachine(rooms, [f1, top, f2])!;
		expect(groups.map((g) => [g.machine, g.main, g.label])).toEqual([
			[top.machine, true, 'Studio'],
			['mx_f1', false, 'Server'],
			['mx_f2', false, 'mx_f2'],
		]);
		expect(groups[0].rooms.map((r) => r.title)).toEqual([
			`Manager ${top.machine}`,
			`${top.machine}/s1`,
		]);
		expect(groups[2].rooms.map((r) => r.title)).toEqual([
			'Manager mx_f2',
			'mx_f2/x',
		]);
	});

	it('puts rooms of unknown machines after known ones with a short id label', () => {
		const rooms = [mgr('mx_unknown0123456789'), mgr(top.machine), mgr('mx_f1')];
		const groups = groupRoomsByMachine(rooms, [top, f1])!;
		expect(groups.map((g) => g.machine)).toEqual([
			top.machine,
			'mx_f1',
			'mx_unknown0123456789',
		]);
		expect(groups[2]).toMatchObject({ main: false, label: 'mx_unknown01' });
	});

	it('uses a null label for a main machine without a name', () => {
		const groups = groupRoomsByMachine(
			[mgr('t'), mgr('s')],
			[{ machine: 't', label: null, role: 'top', local: true }],
		)!;
		expect(groups[0]).toMatchObject({ main: true, label: null });
	});

	it('treats the local machine as main when no role is given', () => {
		const groups = groupRoomsByMachine(
			[mgr('s'), mgr('t')],
			[{ machine: 't', local: true }, { machine: 's' }],
		)!;
		expect(groups.map((g) => [g.machine, g.main])).toEqual([
			['t', true],
			['s', false],
		]);
	});

	it('skips machines without rooms', () => {
		expect(groupRoomsByMachine([mgr(top.machine)], [top, f1, f2])).toBeNull();
	});
});

const dev = (kind: string, state: TrustDevice['state'] = 'trusted') =>
	({
		id: kind + state,
		kind,
		label: kind,
		fp: 'x',
		state,
		is_self: false,
	}) as TrustDevice;

describe('sharedWithPhone', () => {
	it('needs a trusted runner and a trusted phone', () => {
		expect(sharedWithPhone([dev('runner'), dev('mobile')])).toBe(true);
		expect(sharedWithPhone([dev('runner')])).toBe(false);
		expect(sharedWithPhone([dev('mobile')])).toBe(false);
		expect(sharedWithPhone([dev('runner'), dev('mobile', 'pending')])).toBe(
			false,
		);
		expect(sharedWithPhone([dev('runner', 'revoked'), dev('mobile')])).toBe(
			false,
		);
		expect(sharedWithPhone(undefined)).toBe(false);
	});
});
