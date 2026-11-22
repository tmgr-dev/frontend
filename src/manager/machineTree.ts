import type { ManagerMachine, RoomInfo, TrustDevice } from './types';

export interface MachineGroup {
	machine: string;
	main: boolean;
	label: string | null;
	rooms: RoomInfo[];
}

const SHORT_ID = 12;

export const groupRoomsByMachine = (
	rooms: RoomInfo[],
	machines: ManagerMachine[],
): MachineGroup[] | null => {
	if (machines.length === 0) return null;
	const top =
		machines.find((m) => m.role === 'top') ?? machines.find((m) => m.local);
	const known = new Map(machines.map((m) => [m.machine, m]));
	const order = [
		...(top ? [top.machine] : []),
		...machines.map((m) => m.machine).filter((id) => id !== top?.machine),
	];
	const seen = new Set(rooms.map((r) => r.room.machine));
	const unknown = [...seen].filter((id) => !known.has(id));
	const groups = [...order, ...unknown]
		.filter((id) => seen.has(id))
		.map((id): MachineGroup => {
			const own = rooms.filter((r) => r.room.machine === id);
			const main = id === top?.machine;
			const label = known.get(id)?.label || null;
			return {
				machine: id,
				main,
				label: main ? label : (label ?? id.slice(0, SHORT_ID)),
				rooms: [
					...own.filter((r) => r.room.kind === 'manager'),
					...own.filter((r) => r.room.kind !== 'manager'),
				],
			};
		});
	return groups.length > 1 ? groups : null;
};

export const sharedWithPhone = (
	devices: TrustDevice[] | undefined,
): boolean => {
	const trusted = (kind: string) =>
		(devices ?? []).some((d) => d.kind === kind && d.state === 'trusted');
	return trusted('runner') && trusted('mobile');
};
