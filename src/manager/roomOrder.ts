import type { RoomInfo } from './types';
import { roomKey } from './types';

export type SortMode = 'default' | 'recent';
export type OrderStorage = Pick<Storage, 'getItem' | 'setItem'>;

const STORAGE_KEY = 'manager.list.sort';

export const sortRooms = (rooms: RoomInfo[], mode: SortMode): RoomInfo[] => {
	if (mode !== 'recent') return rooms;
	const at = (r: RoomInfo) => r.last?.ts_ms ?? -1;
	return [...rooms].sort((a, b) => at(b) - at(a));
};

export const arrangeByKeys = (
	rooms: RoomInfo[],
	keys: string[],
): RoomInfo[] => {
	const byKey = new Map(rooms.map((r) => [roomKey(r.room), r]));
	const known = keys.flatMap((k) => byKey.get(k) ?? []);
	const placed = new Set(known);
	return [...known, ...rooms.filter((r) => !placed.has(r))];
};

const defaultStorage = (): OrderStorage | null => {
	try {
		return localStorage;
	} catch {
		return null;
	}
};

export const loadSortMode = (
	storage: OrderStorage | null = defaultStorage(),
): SortMode => {
	try {
		return storage?.getItem(STORAGE_KEY) === 'recent' ? 'recent' : 'default';
	} catch {
		return 'default';
	}
};

export const saveSortMode = (
	mode: SortMode,
	storage: OrderStorage | null = defaultStorage(),
): void => {
	try {
		storage?.setItem(STORAGE_KEY, mode);
	} catch {
		return;
	}
};
