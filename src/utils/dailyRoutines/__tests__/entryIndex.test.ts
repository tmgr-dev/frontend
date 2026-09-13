import { indexEntriesByDate } from '../entryIndex';
it('groups dates once without changing order, identity or source array', () => {
	const entries = [
		{ date: '2026-09-13', id: 2 },
		{ date: '2026-09-14', id: 3 },
		{ date: '2026-09-13', id: 1 },
	];
	const index = indexEntriesByDate(entries);
	expect(index.get('2026-09-13')).toEqual([entries[0], entries[2]]);
	expect(index.get('2026-09-13')![0]).toBe(entries[0]);
	expect(index.get('missing')).toBeUndefined();
	expect(entries.map((e) => e.id)).toEqual([2, 3, 1]);
});
