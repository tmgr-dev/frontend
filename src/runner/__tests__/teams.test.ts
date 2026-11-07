import type { RunnerSession } from '../api';
import { groupTeams, teamLabel } from '../teams';

const make = (id: string, team?: RunnerSession['team']): RunnerSession => ({
	id,
	name: id,
	profile: null,
	state: { kind: 'working', sinceMs: 0, detail: null },
	createdMs: 0,
	...(team ? { team } : {}),
});

const lead = (id: string, of = 't1') => make(id, { of, role: 'lead' });
const worker = (id: string, n: number, of = 't1') =>
	make(id, { of, role: 'worker', worker: n });
const ids = (rows: { session: RunnerSession }[]) =>
	rows.map((r) => r.session.id);

describe('teamLabel', () => {
	it('labels lead and workers, and nothing without a team', () => {
		expect(teamLabel(lead('a'))).toBe('lead');
		expect(teamLabel(worker('b', 2))).toBe('w2');
		expect(teamLabel(make('c'))).toBeNull();
		expect(teamLabel({ ...make('d'), team: null })).toBeNull();
	});
});

describe('groupTeams', () => {
	it('keeps sessions without a team in place with no label', () => {
		const rows = groupTeams([make('a'), make('b')]);
		expect(ids(rows)).toEqual(['a', 'b']);
		expect(rows.map((r) => r.label)).toEqual([null, null]);
	});

	it('puts the lead first, then workers in order, at the first member position', () => {
		const rows = groupTeams([
			make('x'),
			worker('w2', 2),
			make('y'),
			worker('w1', 1),
			lead('l'),
		]);
		expect(ids(rows)).toEqual(['x', 'l', 'w1', 'w2', 'y']);
		expect(rows.map((r) => r.label)).toEqual([null, 'lead', 'w1', 'w2', null]);
	});

	it('still shows a worker whose lead is gone', () => {
		const rows = groupTeams([worker('w2', 2), worker('w1', 1)]);
		expect(ids(rows)).toEqual(['w1', 'w2']);
		expect(rows.map((r) => r.label)).toEqual(['w1', 'w2']);
	});

	it('keeps separate teams apart', () => {
		const rows = groupTeams([
			worker('a1', 1, 'a'),
			worker('b1', 1, 'b'),
			lead('al', 'a'),
			lead('bl', 'b'),
		]);
		expect(ids(rows)).toEqual(['al', 'a1', 'bl', 'b1']);
	});
});
