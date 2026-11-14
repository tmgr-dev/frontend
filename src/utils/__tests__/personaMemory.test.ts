import {
	isCheckByStale,
	SELF_MEMORY_MAX_BYTES,
	selfMemoryTooLong,
	totalTopicCount,
} from '../personaMemory';

describe('isCheckByStale', () => {
	const now = new Date(2026, 10, 14, 12, 0, 0);

	it('is false without a date', () => {
		expect(isCheckByStale(null, now)).toBe(false);
	});

	it('is true for a past day', () => {
		expect(isCheckByStale('2026-11-13', now)).toBe(true);
	});

	it('is false for today and the future', () => {
		expect(isCheckByStale('2026-11-14', now)).toBe(false);
		expect(isCheckByStale('2026-12-01', now)).toBe(false);
	});
});

describe('selfMemoryTooLong', () => {
	it('counts bytes, not characters', () => {
		expect(SELF_MEMORY_MAX_BYTES).toBe(4096);
		expect(selfMemoryTooLong('a'.repeat(4096))).toBe(false);
		expect(selfMemoryTooLong('я'.repeat(2049))).toBe(true);
	});
});

describe('totalTopicCount', () => {
	it('sums the per-topic counts', () => {
		expect(totalTopicCount({ notes: 2, people: 3 })).toBe(5);
		expect(totalTopicCount({})).toBe(0);
	});
});
