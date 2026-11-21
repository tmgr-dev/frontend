import {
	MISMATCH_ERROR,
	createDecisionActionRunner,
	type ClaimedAction,
	type PendingAction,
} from '../useDecisionActions';

const commentAction = (over: Record<string, unknown> = {}) => ({
	kind: 'comment' as const,
	payload: {
		origin: 'https://api.tmgr.dev',
		user_id: 5,
		task_id: 42,
		task_label: 'TM-42: x',
		text: 'hello',
		...over,
	},
});

const taskAction = (over: Record<string, unknown> = {}) => ({
	kind: 'task' as const,
	payload: {
		origin: 'https://api.tmgr.dev',
		user_id: 5,
		workspace_id: 3,
		category_id: 8,
		status_id: 9,
		title: 'New',
		description: 'Body',
		...over,
	},
});

const setup = (
	pending: PendingAction[],
	opts: { origin?: string | null; userId?: number | null } = {},
) => {
	const calls: string[] = [];
	const finished: { runId: string; result: any }[] = [];
	const claimed = new Set<string>();
	const deps = {
		invoke: jest.fn(async (cmd: string, args?: any) => {
			calls.push(cmd);
			if (cmd === 'decisions_pending_actions') {
				return pending.filter((p) => !claimed.has(p.run_id));
			}
			if (cmd === 'decisions_claim_action') {
				if (claimed.has(args.runId)) throw new Error('already claimed');
				claimed.add(args.runId);
				const item = pending.find((p) => p.run_id === args.runId)!;
				return {
					run_id: item.run_id,
					card_id: item.card_id,
					action: item.action,
				} as ClaimedAction;
			}
			if (cmd === 'decisions_finish_action') {
				finished.push({ runId: args.runId, result: args.result });
			}
			return undefined;
		}),
		postComment: jest.fn(async (_id: number, _text: string) => ({})),
		createTask: jest.fn(async (_fields: Record<string, unknown>) => ({
			id: 77,
		})),
		taskWorkspaceId: jest.fn(async (_id: number) => 3 as number | null),
		workspaceCode: jest.fn((id: number) => (id === 3 ? 'ws' : null)),
		webOrigin: 'https://tmgr.dev',
		origin: opts.origin === undefined ? 'https://api.tmgr.dev' : opts.origin,
		userId: () => (opts.userId === undefined ? 5 : opts.userId),
	};
	return { deps, calls, finished, runner: createDecisionActionRunner(deps) };
};

const item = (action: any, run_id = 'r1'): PendingAction => ({
	run_id,
	card_id: 'c1',
	action,
});

describe('decision action runner', () => {
	it('claims, posts the claimed comment and finishes with the task url', async () => {
		const { deps, calls, finished, runner } = setup([item(commentAction())]);
		await runner.run();
		expect(calls).toEqual([
			'decisions_pending_actions',
			'decisions_claim_action',
			'decisions_finish_action',
		]);
		expect(deps.postComment).toHaveBeenCalledWith(42, 'hello');
		expect(finished).toEqual([
			{
				runId: 'r1',
				result: { ok: true, url: 'https://tmgr.dev/ws/tasks/42' },
			},
		]);
	});

	it('creates a task with the exact fields and reports its url', async () => {
		const { deps, finished, runner } = setup([item(taskAction())]);
		await runner.run();
		expect(deps.createTask).toHaveBeenCalledWith({
			title: 'New',
			description: 'Body',
			workspace_id: 3,
			status_id: 9,
			status: 'created',
			is_daily_routine: false,
			project_category_id: 8,
		});
		expect(finished[0].result).toEqual({
			ok: true,
			url: 'https://tmgr.dev/ws/tasks/77',
		});
	});

	it('omits optional task fields and the url when the workspace code is unknown', async () => {
		const { deps, finished, runner } = setup([
			item(taskAction({ category_id: null, status_id: null, workspace_id: 4 })),
		]);
		deps.workspaceCode.mockReturnValue(null);
		await runner.run();
		const fields = deps.createTask.mock.calls[0][0];
		expect(fields).not.toHaveProperty('project_category_id');
		expect(fields).not.toHaveProperty('status_id');
		expect(finished[0].result).toEqual({ ok: true });
	});

	it('refuses on another server without posting', async () => {
		const { deps, calls, finished, runner } = setup([item(commentAction())], {
			origin: 'http://localhost:8080',
		});
		await runner.run();
		expect(calls).toContain('decisions_claim_action');
		expect(deps.postComment).not.toHaveBeenCalled();
		expect(finished[0].result).toEqual({ ok: false, error: MISMATCH_ERROR });
	});

	it('refuses on another account without posting', async () => {
		const { deps, finished, runner } = setup([item(taskAction())], {
			userId: 6,
		});
		await runner.run();
		expect(deps.createTask).not.toHaveBeenCalled();
		expect(finished[0].result).toEqual({ ok: false, error: MISMATCH_ERROR });
	});

	it('refuses when the signed-in user is not known yet', async () => {
		const { deps, finished, runner } = setup([item(commentAction())], {
			userId: null,
		});
		await runner.run();
		expect(deps.postComment).not.toHaveBeenCalled();
		expect(finished[0].result.ok).toBe(false);
	});

	it('finishes with the API error message and never retries', async () => {
		const { deps, finished, runner } = setup([item(commentAction())]);
		deps.postComment.mockRejectedValue({
			response: { data: { message: 'Forbidden' } },
		});
		await runner.run();
		await runner.run();
		expect(deps.postComment).toHaveBeenCalledTimes(1);
		expect(finished).toEqual([
			{ runId: 'r1', result: { ok: false, error: 'Forbidden' } },
		]);
	});

	it('claims each run once when runs overlap', async () => {
		const { calls, runner } = setup([item(commentAction())]);
		await Promise.all([runner.run(), runner.run()]);
		expect(calls.filter((c) => c === 'decisions_claim_action')).toHaveLength(1);
	});

	it('skips a run that another window already claimed', async () => {
		const { deps, finished, runner } = setup([item(commentAction())]);
		deps.invoke.mockImplementation(async (cmd: string) => {
			if (cmd === 'decisions_pending_actions') return [item(commentAction())];
			throw new Error('already claimed');
		});
		await runner.run();
		expect(deps.postComment).not.toHaveBeenCalled();
		expect(finished).toEqual([]);
	});

	it('ignores kinds the window does not execute', async () => {
		const { calls, runner } = setup([
			item({ kind: 'tag', payload: { repo: 'o/r' } }),
		]);
		await runner.run();
		expect(calls).toEqual(['decisions_pending_actions']);
	});
});
