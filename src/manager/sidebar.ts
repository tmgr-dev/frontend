import { roomKey, type RoomInfo } from './types';

export interface StateCounts {
	working: number;
	waiting: number;
	failed: number;
}

export interface NeedsYouRow {
	key: string;
	tag: 'permission' | 'question' | 'failed';
	title: string;
}

const sessionsOf = (rooms: RoomInfo[]): RoomInfo[] =>
	rooms.filter((r) => r.room.kind !== 'manager');

export const rowTitle = (info: RoomInfo): string =>
	info.task && info.agent ? `${info.agent} · ${info.task}` : info.title;

export const stateCounts = (rooms: RoomInfo[]): StateCounts => {
	const counts: StateCounts = { working: 0, waiting: 0, failed: 0 };
	for (const r of sessionsOf(rooms)) {
		if (r.state === 'working' || r.state === 'failed') counts[r.state] += 1;
		else if (
			r.state === 'waiting' ||
			r.state === 'blocked' ||
			r.state === 'asking'
		)
			counts.waiting += 1;
	}
	return counts;
};

const NEED_TAG: Record<string, NeedsYouRow['tag']> = {
	blocked: 'permission',
	asking: 'question',
	failed: 'failed',
};
const NEED_ORDER: NeedsYouRow['tag'][] = ['permission', 'question', 'failed'];

export const needsYou = (rooms: RoomInfo[]): NeedsYouRow[] => {
	const rows: NeedsYouRow[] = [];
	for (const r of sessionsOf(rooms)) {
		const tag = r.state ? NEED_TAG[r.state] : undefined;
		if (tag) rows.push({ key: roomKey(r.room), tag, title: rowTitle(r) });
	}
	return rows.sort(
		(a, b) => NEED_ORDER.indexOf(a.tag) - NEED_ORDER.indexOf(b.tag),
	);
};
