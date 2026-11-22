import { roomKey, type RoomInfo } from './types';

export interface StateCounts {
	working: number;
	waiting: number;
	failed: number;
}

export interface NeedsYouRow {
	key: string;
	tag: 'waiting' | 'failed';
	title: string;
}

const sessionsOf = (rooms: RoomInfo[]): RoomInfo[] =>
	rooms.filter((r) => r.room.kind !== 'manager');

export const rowTitle = (info: RoomInfo): string =>
	info.task && info.agent ? `${info.agent} · ${info.task}` : info.title;

export const stateCounts = (rooms: RoomInfo[]): StateCounts => {
	const counts: StateCounts = { working: 0, waiting: 0, failed: 0 };
	for (const r of sessionsOf(rooms)) {
		if (r.state === 'working' || r.state === 'waiting' || r.state === 'failed')
			counts[r.state] += 1;
	}
	return counts;
};

export const needsYou = (rooms: RoomInfo[]): NeedsYouRow[] => {
	const rows: NeedsYouRow[] = [];
	for (const r of sessionsOf(rooms)) {
		if (r.state === 'waiting' || r.state === 'failed')
			rows.push({ key: roomKey(r.room), tag: r.state, title: rowTitle(r) });
	}
	return rows.sort((a, b) =>
		a.tag === b.tag ? 0 : a.tag === 'waiting' ? -1 : 1,
	);
};
