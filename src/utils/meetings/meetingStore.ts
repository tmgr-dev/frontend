import type { MeetingRecord } from './meetingCreate';

export const RETENTION_DAYS = 30;
const KEY = 'tmgr.meetings.v1';
const DAY_MS = 86_400_000;

export interface KeyValueStore {
	getItem(key: string): string | null;
	setItem(key: string, value: string): void;
}

const read = (kv: KeyValueStore): MeetingRecord[] => {
	try {
		const parsed = JSON.parse(kv.getItem(KEY) ?? '[]');
		return Array.isArray(parsed) ? parsed : [];
	} catch {
		return [];
	}
};

const write = (kv: KeyValueStore, records: MeetingRecord[]): void => {
	try {
		kv.setItem(KEY, JSON.stringify(records));
	} catch {
		throw new Error(
			'Not enough local storage to keep this transcript. Delete an older meeting and try again.',
		);
	}
};

export const loadMeetings = (
	kv: KeyValueStore,
	now: number,
): MeetingRecord[] => {
	const all = read(kv);
	const kept = all.filter(
		(record) => now - record.importedAt < RETENTION_DAYS * DAY_MS,
	);
	if (kept.length !== all.length) write(kv, kept);
	return kept;
};

export const saveMeeting = (kv: KeyValueStore, record: MeetingRecord): void => {
	const all = read(kv);
	const index = all.findIndex((item) => item.id === record.id);
	if (index >= 0) all[index] = record;
	else all.push(record);
	write(kv, all);
};

export const deleteMeeting = (kv: KeyValueStore, id: string): void => {
	write(
		kv,
		read(kv).filter((record) => record.id !== id),
	);
};

export const newMeetingId = (): string => crypto.randomUUID();
