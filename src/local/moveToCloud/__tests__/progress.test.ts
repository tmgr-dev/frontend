import { createMover, type MoveView } from '../flow';
import { phaseRow, summarize } from '../progress';
import { harness, verifiedStatus } from './helpers';

describe('progress', () => {
	it('maps server phases to checklist rows', () => {
		expect(phaseRow('tasks')).toBe('tasks');
		expect(phaseRow('link_rewrite')).toBe('links');
		expect(phaseRow('agent_runs')).toBe('routines');
		expect(phaseRow('pages')).toBe('pages');
		expect(phaseRow('page_versions')).toBe('pages');
		expect(phaseRow('verify')).toBe('publish');
		expect(phaseRow(null)).toBe('statuses');
	});

	it('maps every real phase name to the documented row', () => {
		const expected: Record<string, string> = {
			staging: 'statuses',
			personas: 'statuses',
			statuses: 'statuses',
			categories: 'statuses',
			tasks: 'tasks',
			relations: 'tasks',
			comments: 'comments',
			pages: 'pages',
			page_versions: 'pages',
			routines: 'routines',
			files: 'files',
			assignments: 'routines',
			agent_runs: 'routines',
			link_rewrite: 'links',
			modules: 'routines',
			verify: 'publish',
		};
		for (const [phase, row] of Object.entries(expected)) {
			expect([phase, phaseRow(phase)]).toEqual([phase, row]);
		}
	});

	it('does not regress rows while phases run out of row order', () => {
		const rowsAt = (phase: string) =>
			Object.fromEntries(
				summarize({
					code: 'loc',
					job: {
						step: 'committed',
						excluded: [],
					} as unknown as MoveView['job'],
					published: null,
					status: { ...verifiedStatus(), status: 'running', phase },
					filesDone: 0,
					filesTotal: 0,
					busy: true,
					canPublishAnyway: false,
					failedOnServer: false,
					accountMismatch: false,
				}).rows.map((r) => [r.id, r.state]),
			);
		expect(rowsAt('relations')).toMatchObject({
			tasks: 'active',
			comments: 'pending',
			routines: 'pending',
		});
		expect(rowsAt('page_versions')).toMatchObject({
			comments: 'done',
			pages: 'active',
			files: 'pending',
		});
		expect(rowsAt('link_rewrite')).toMatchObject({
			files: 'done',
			pages: 'done',
			links: 'active',
		});
	});

	it('shows backup done and bundle active before the server runs', async () => {
		const h = harness();
		h.api.presignFile.mockRejectedValue(new Error('stop'));
		const mover = createMover(h.deps);
		const dry = await mover.dryRun('loc', 'Local');
		const view = await mover.start('loc', 'Local', dry, {
			cloudName: 'C',
			personas: [],
		});
		expect(h.api.commit).not.toHaveBeenCalled();
		const summary = summarize(view);
		expect(summary.rows[0].state).toBe('done');
		expect(summary.rows[1].state).toBe('done');
		expect(summary.rows.find((r) => r.id === 'files')?.state).toBe('active');
		expect(summary.rows.find((r) => r.id === 'files')?.label).toBe(
			'Files 0 of 2 · 2 skipped (see report)',
		);
	});

	it('follows the server phase with counts and a percent', () => {
		const job: MoveView['job'] = {
			step: 'committed',
			excluded: [
				{
					id: 1,
					name: 'x',
					key: 'x',
					size: 1,
					reason: 'mime',
					resolution: null,
				},
			],
		} as MoveView['job'];
		const summary = summarize({
			code: 'loc',
			job,
			published: null,
			status: {
				...verifiedStatus(),
				status: 'running',
				phase: 'comments',
				done: 50,
				total: 100,
			},
			filesDone: 0,
			filesTotal: 0,
			busy: true,
			canPublishAnyway: false,
			failedOnServer: false,
			accountMismatch: false,
		});
		const states = summary.rows.map((r) => r.state);
		expect(states).toEqual([
			'done',
			'done',
			'done',
			'done',
			'active',
			'pending',
			'pending',
			'pending',
			'pending',
			'pending',
		]);
		expect(summary.rows[4].label).toBe('Comments 50 of 100');
		expect(summary.step).toBe(5);
		expect(summary.percent).toBe(45);
	});

	it('marks a finished move as 100%', () => {
		const summary = summarize({
			code: 'loc',
			job: null,
			published: null,
			status: null,
			filesDone: 0,
			filesTotal: 0,
			busy: false,
			canPublishAnyway: false,
			failedOnServer: false,
			accountMismatch: false,
		});
		expect(summary.percent).toBe(100);
	});
});
