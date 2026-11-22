import type { ManagerMachine, RoomInfo } from './types';

export interface HierarchyNode {
	machine: string;
	label: string;
	manager: RoomInfo | null;
	main: boolean;
	roles: boolean;
	sessions: RoomInfo[];
}

export const HANDOFF_TOKENS = 180_000;

export const needsHandoff = (tokens?: number): boolean =>
	tokens !== undefined && tokens > HANDOFF_TOKENS;

export const buildHierarchy = (
	rooms: RoomInfo[],
	machines: ManagerMachine[] = [],
): HierarchyNode[] => {
	const known = new Map(machines.map((m) => [m.machine, m]));
	const top =
		machines.find((m) => m.role === 'top') ?? machines.find((m) => m.local);
	const order: string[] = [];
	for (const r of rooms)
		if (!order.includes(r.room.machine)) order.push(r.room.machine);
	const mainId =
		top && order.includes(top.machine)
			? top.machine
			: machines.length
				? rooms.find(
						(r) =>
							r.room.kind === 'manager' &&
							known.get(r.room.machine)?.role !== 'sub',
					)?.room.machine
				: (rooms.find((r) => r.room.kind === 'manager')?.room.machine ??
					order[0]);
	const sorted = [
		...order.filter((id) => id === mainId),
		...order.filter((id) => id !== mainId),
	];
	return sorted.map((machine) => {
		const own = rooms.filter((r) => r.room.machine === machine);
		return {
			machine,
			label: known.get(machine)?.label || machine,
			manager: own.find((r) => r.room.kind === 'manager') ?? null,
			main: machine === mainId,
			roles: machines.length > 0,
			sessions: own.filter((r) => r.room.kind !== 'manager'),
		};
	});
};
