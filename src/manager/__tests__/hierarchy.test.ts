import { buildHierarchy, needsHandoff } from '../hierarchy';
import type { ManagerMachine, RoomInfo } from '../types';

const mgr = (machine: string): RoomInfo => ({
	room: { kind: 'manager', machine },
	title: 'Manager',
});
const ses = (machine: string, session: string): RoomInfo => ({
	room: { kind: 'session', machine, session },
	title: session,
	agent: 'claude',
});

describe('buildHierarchy', () => {
	it('puts the top machine first with its manager and sessions', () => {
		const machines: ManagerMachine[] = [
			{ machine: 'studio', role: 'sub' },
			{ machine: 'mac', role: 'top', label: 'This Mac' },
		];
		const nodes = buildHierarchy(
			[mgr('studio'), ses('studio', 's2'), mgr('mac'), ses('mac', 's1')],
			machines,
		);
		expect(nodes.map((n) => n.machine)).toEqual(['mac', 'studio']);
		expect(nodes[0]).toMatchObject({ main: true, label: 'This Mac' });
		expect(nodes[0].manager?.room.machine).toBe('mac');
		expect(nodes[1].main).toBe(false);
		expect(nodes[1].sessions).toHaveLength(1);
	});

	it('falls back to the first manager room without machine data', () => {
		const nodes = buildHierarchy([ses('a', 's0'), mgr('b'), mgr('c')]);
		expect(nodes[0]).toMatchObject({ machine: 'b', main: true });
		expect(nodes.find((n) => n.machine === 'a')?.manager).toBeNull();
	});

	it('is empty for no rooms', () => {
		expect(buildHierarchy([])).toEqual([]);
	});
});

describe('needsHandoff', () => {
	it('triggers only above 180k', () => {
		expect(needsHandoff(180_000)).toBe(false);
		expect(needsHandoff(180_001)).toBe(true);
		expect(needsHandoff(undefined)).toBe(false);
	});
});
