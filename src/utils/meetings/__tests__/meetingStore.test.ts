import type { MeetingRecord } from '../meetingCreate';
import {
	deleteMeeting,
	loadMeetings,
	newMeetingId,
	RETENTION_DAYS,
	saveMeeting,
} from '../meetingStore';

const DAY = 86_400_000;

const memory = () => {
	const data = new Map<string, string>();
	return {
		data,
		getItem: (k: string) => data.get(k) ?? null,
		setItem: (k: string, v: string) => void data.set(k, v),
		removeItem: (k: string) => void data.delete(k),
	};
};

const record = (id: string, importedAt: number): MeetingRecord => ({
	id,
	title: id,
	date: '2026-11-15',
	importedAt,
	transcript: 'x',
	participantsInformed: true,
	items: [],
	created: {},
	pageId: null,
});

describe('meetingStore', () => {
	const now = 100 * DAY;

	it('keeps transcripts for 30 days and purges older ones on load', () => {
		const kv = memory();
		saveMeeting(kv, record('fresh', now - 29 * DAY));
		saveMeeting(kv, record('old', now - RETENTION_DAYS * DAY));

		const loaded = loadMeetings(kv, now);

		expect(RETENTION_DAYS).toBe(30);
		expect(loaded.map((m) => m.id)).toEqual(['fresh']);
		expect(JSON.parse(kv.getItem('tmgr.meetings.v1')!)).toHaveLength(1);
	});

	it('upserts by id and deletes', () => {
		const kv = memory();
		saveMeeting(kv, record('a', now));
		saveMeeting(kv, { ...record('a', now), title: 'renamed' });
		saveMeeting(kv, record('b', now));

		expect(loadMeetings(kv, now).map((m) => m.title)).toEqual(['renamed', 'b']);

		deleteMeeting(kv, 'a');
		expect(loadMeetings(kv, now).map((m) => m.id)).toEqual(['b']);
	});

	it('survives corrupt storage', () => {
		const kv = memory();
		kv.setItem('tmgr.meetings.v1', '{not json');

		expect(loadMeetings(kv, now)).toEqual([]);
	});

	it('explains a full storage', () => {
		const kv = memory();
		kv.setItem = () => {
			throw new Error('QuotaExceededError');
		};

		expect(() => saveMeeting(kv, record('a', now))).toThrow(/storage/i);
	});

	it('makes unique ids', () => {
		expect(newMeetingId()).not.toBe(newMeetingId());
	});
});
