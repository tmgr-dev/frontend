import {
	buildArchivedStatusSets,
	isArchivedTask,
	ArchivedStatusSets,
} from '../archivedTasks';

describe('buildArchivedStatusSets', () => {
	it('returns empty sets for null/undefined input', () => {
		for (const input of [null, undefined]) {
			const sets = buildArchivedStatusSets(input);
			expect(sets.ids.size).toBe(0);
			expect(sets.names.size).toBe(0);
		}
	});

	it('returns empty sets when no status is archived', () => {
		const sets = buildArchivedStatusSets([
			{ id: 1, name: 'To do', type: 'open' },
			{ id: 2, name: 'Done', type: 'completed' },
		]);
		expect(sets.ids.size).toBe(0);
		expect(sets.names.size).toBe(0);
	});

	it('collects id and name only for archived-type statuses', () => {
		const sets = buildArchivedStatusSets([
			{ id: 1, name: 'In progress', type: 'open' },
			{ id: 7, name: 'Archived', type: 'archived' },
			{ id: 9, name: 'Old archive', type: 'archived' },
		]);
		expect([...sets.ids].sort()).toEqual([7, 9]);
		expect([...sets.names].sort()).toEqual(['Archived', 'Old archive']);
	});

	it('coerces string ids to numbers', () => {
		const sets = buildArchivedStatusSets([
			{ id: '42', name: 'Archived', type: 'archived' },
		]);
		expect(sets.ids.has(42)).toBe(true);
	});

	it('skips non-numeric ids but still keeps the name', () => {
		const sets = buildArchivedStatusSets([
			{ id: 'not-a-number', name: 'Archived', type: 'archived' },
		]);
		expect(sets.ids.size).toBe(0);
		expect(sets.names.has('Archived')).toBe(true);
	});

	it('skips missing id and missing name without throwing', () => {
		const sets = buildArchivedStatusSets([
			{ type: 'archived' },
			{ id: null, name: null, type: 'archived' },
			{ id: 5, type: 'archived' },
			{ name: 'NamedOnly', type: 'archived' },
		]);
		expect([...sets.ids]).toEqual([5]);
		expect([...sets.names]).toEqual(['NamedOnly']);
	});
});

describe('isArchivedTask', () => {
	const sets: ArchivedStatusSets = {
		ids: new Set([7, 9]),
		names: new Set(['Archived']),
	};

	it('returns false for null/undefined task', () => {
		expect(isArchivedTask(null, sets)).toBe(false);
		expect(isArchivedTask(undefined, sets)).toBe(false);
	});

	it('matches by numeric status_id', () => {
		expect(isArchivedTask({ status_id: 7, status: 'whatever' }, sets)).toBe(
			true,
		);
	});

	it('matches by string status_id (realtime/optimistic payloads)', () => {
		expect(isArchivedTask({ status_id: '9', status: 'whatever' }, sets)).toBe(
			true,
		);
	});

	it('falls back to status name when status_id is absent', () => {
		expect(isArchivedTask({ status: 'Archived' }, sets)).toBe(true);
	});

	it('falls back to status name when status_id is non-numeric', () => {
		expect(
			isArchivedTask({ status_id: 'abc', status: 'Archived' }, sets),
		).toBe(true);
	});

	it('returns false for an active task', () => {
		expect(
			isArchivedTask({ status_id: 1, status: 'In progress' }, sets),
		).toBe(false);
	});

	it('returns false when nothing matches and status_id is unknown', () => {
		expect(isArchivedTask({ status_id: 123, status: 'Done' }, sets)).toBe(
			false,
		);
	});

	it('returns false against empty sets', () => {
		const empty: ArchivedStatusSets = { ids: new Set(), names: new Set() };
		expect(isArchivedTask({ status_id: 7, status: 'Archived' }, empty)).toBe(
			false,
		);
	});

	it('integrates with buildArchivedStatusSets end to end', () => {
		const built = buildArchivedStatusSets([
			{ id: 3, name: 'Active', type: 'open' },
			{ id: 8, name: 'Archived', type: 'archived' },
		]);
		expect(isArchivedTask({ status_id: 8 }, built)).toBe(true);
		expect(isArchivedTask({ status_id: '8' }, built)).toBe(true);
		expect(isArchivedTask({ status: 'Archived' }, built)).toBe(true);
		expect(isArchivedTask({ status_id: 3, status: 'Active' }, built)).toBe(
			false,
		);
	});
});
