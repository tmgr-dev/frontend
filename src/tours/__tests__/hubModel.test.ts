import { buildHubModel, moduleOffMessage, tourForRoute } from '../hubModel';
import type { HubItem } from '../useTours';
import type { TourDef, TourState } from '../types';

const tour = (id: string, group: TourDef['group'], extra: Partial<TourDef> = {}): TourDef => ({
	id,
	version: 1,
	since: '3.5.0',
	group,
	title: id,
	minutes: 2,
	triggers: [{ kind: 'hub' }],
	steps: [
		{ id: 'a', type: 'tip', title: 'A', body: 'a' },
		{ id: 'b', type: 'tip', title: 'B', body: 'b' },
		{ id: 'c', type: 'tip', title: 'C', body: 'c' },
	],
	...extra,
});

const state = (status: TourState['status'], step_id: string | null, version = 1): TourState => ({
	status,
	step_id,
	version,
	generation: 0,
});

const item = (t: TourDef, s: TourState | null = null, extra: Partial<HubItem> = {}): HubItem => ({
	tour: t,
	state: s,
	available: true,
	...extra,
});

const context = { canManageModules: true, isModuleEnabled: () => true };

describe('buildHubModel', () => {
	it('groups by catalog order and counts done over listed tours', () => {
		const model = buildHubModel(
			[
				item(tour('t1', 'tasks'), state('completed', null)),
				item(tour('g1', 'start'), state('completed', null)),
				item(tour('g2', 'start')),
			],
			context,
		);
		expect(model.groups.map((g) => g.title)).toEqual(['Getting started', 'Tasks & board']);
		expect(model.done).toBe(2);
		expect(model.total).toBe(3);
		expect(model.groups[0].rows.map((r) => r.action)).toEqual(['done', 'start']);
	});

	it('shows resume with the step position and falls back to step 1 for an unknown step', () => {
		const model = buildHubModel(
			[
				item(tour('r1', 'agents'), state('paused', 'b')),
				item(tour('r2', 'agents'), state('in_progress', 'gone')),
			],
			context,
		);
		expect(model.groups[0].rows.map((r) => [r.action, r.progress])).toEqual([
			['resume', 'step 2 of 3'],
			['resume', 'step 1 of 3'],
		]);
	});

	it('offers a skipped tour again and marks a completed older version as updated', () => {
		const model = buildHubModel(
			[
				item(tour('s', 'time'), state('skipped', 'a')),
				item(tour('u', 'time', { version: 2 }), state('completed', null, 1)),
			],
			context,
		);
		expect(model.groups[0].rows.map((r) => [r.action, r.updated])).toEqual([
			['start', false],
			['start', true],
		]);
		expect(model.done).toBe(0);
	});

	it('explains a module-off tour by who can fix it and keeps it out of the progress count', () => {
		const off = item(tour('m', 'graph', { requires: { modules: ['graph'] } }), null, {
			available: false,
			unavailableReason: 'module_off',
		});
		const owner = buildHubModel([off], { canManageModules: true, isModuleEnabled: () => false });
		expect(owner.groups[0].rows[0]).toMatchObject({
			action: 'none',
			disabledMessage: 'Turn on Graph in Settings -> Modules',
		});
		expect(owner.total).toBe(0);
		const member = buildHubModel([off], { canManageModules: false, isModuleEnabled: () => false });
		expect(member.groups[0].rows[0].disabledMessage).toBe('Ask the workspace owner to turn on Graph');
	});

	it('hides tours unavailable for platform, account or role reasons', () => {
		const model = buildHubModel(
			[
				item(tour('p', 'desktop'), null, { available: false, unavailableReason: 'platform' }),
				item(tour('o', 'workspace'), null, { available: false, unavailableReason: 'role' }),
			],
			context,
		);
		expect(model.groups).toEqual([]);
	});

	it('humanises dotted module keys', () => {
		expect(moduleOffMessage('task.assignees', false)).toBe('Ask the workspace owner to turn on Task assignees');
	});
});

describe('tourForRoute', () => {
	const board = tour('t.board', 'tasks', { triggers: [{ kind: 'first_visit', routeName: 'WorkspaceBoard' }] });

	it('finds the available tour that opens on the current route', () => {
		expect(tourForRoute([item(tour('x', 'start')), item(board)], 'WorkspaceBoard')?.tour.id).toBe('t.board');
	});

	it('ignores unavailable tours and unknown routes', () => {
		expect(tourForRoute([item(board, null, { available: false })], 'WorkspaceBoard')).toBeNull();
		expect(tourForRoute([item(board)], 'WorkspaceList')).toBeNull();
		expect(tourForRoute([item(board)], undefined)).toBeNull();
	});
});
