import {
	BACKOFF_MS,
	createDecisionTaskCommentSync,
	type PendingTaskComment,
} from '../useDecisionTaskComments';

const item = (over: Partial<PendingTaskComment> = {}): PendingTaskComment => ({
	card_id: 'c_1',
	seq: 1,
	task_id: 42,
	origin: 'https://tmgr.dev',
	workspace: 'ws',
	body: 'answer [decision c_1#1]',
	...over,
});

const httpError = (status: number) => ({ response: { status } });

const setup = (pending: PendingTaskComment[] = [item()]) => {
	let clock = 1_000_000;
	const marked: string[] = [];
	const skipped: string[] = [];
	const deps = {
		invoke: jest.fn(async (cmd: string, args?: any) => {
			if (cmd === 'decisions_pending_task_comments') {
				return pending.filter((p) => {
					const key = `${p.card_id}#${p.seq}`;
					return (
						!marked.includes(key) &&
						!skipped.some((s) => s.startsWith(`${key}:`))
					);
				});
			}
			marked.push(`${args.cardId}#${args.seq}`);
			return undefined;
		}),
		fetchComments: jest.fn(
			async (_taskId: number) => [] as { message: string }[],
		),
		postComment: jest.fn(async (_taskId: number, _message: string) => ({})),
		skip: jest.fn(async (cardId: string, seq: number, reason: string) => {
			skipped.push(`${cardId}#${seq}:${reason}`);
		}),
		taskWorkspaceCode: jest.fn(
			async (_taskId: number) => 'ws' as string | null,
		),
		now: () => clock,
		origin: 'https://tmgr.dev',
	};
	const advance = (ms: number) => {
		clock += ms;
	};
	return {
		deps,
		marked,
		skipped,
		advance,
		sync: createDecisionTaskCommentSync(deps),
	};
};

describe('decision task comment sync', () => {
	it('posts when the task belongs to the workspace of the card link', async () => {
		const { deps, marked, sync } = setup();
		await sync.run();
		expect(deps.taskWorkspaceCode).toHaveBeenCalledWith(42);
		expect(deps.postComment).toHaveBeenCalledTimes(1);
		expect(marked).toEqual(['c_1#1']);
	});

	it('skips permanently when the task is in another workspace and logs it', async () => {
		const warn = jest.spyOn(console, 'warn').mockImplementation(() => {});
		const { deps, marked, skipped, advance, sync } = setup();
		deps.taskWorkspaceCode.mockResolvedValue('other');
		await sync.run();
		expect(deps.postComment).not.toHaveBeenCalled();
		expect(deps.fetchComments).not.toHaveBeenCalled();
		expect(marked).toEqual([]);
		expect(deps.skip).toHaveBeenCalledTimes(1);
		expect(skipped).toEqual(['c_1#1:workspace mismatch']);
		expect(warn).toHaveBeenCalledTimes(1);
		advance(BACKOFF_MS + 1);
		await sync.run();
		await sync.run();
		expect(deps.taskWorkspaceCode).toHaveBeenCalledTimes(1);
		expect(skipped).toHaveLength(1);
		warn.mockRestore();
	});

	it('does not post when the task workspace cannot be resolved', async () => {
		const warn = jest.spyOn(console, 'warn').mockImplementation(() => {});
		const { deps, marked, skipped, advance, sync } = setup();
		deps.taskWorkspaceCode.mockResolvedValue(null);
		await sync.run();
		expect(deps.postComment).not.toHaveBeenCalled();
		expect(marked).toEqual([]);
		expect(deps.skip).not.toHaveBeenCalled();
		expect(skipped).toEqual([]);
		await sync.run();
		expect(deps.taskWorkspaceCode).toHaveBeenCalledTimes(1);
		advance(BACKOFF_MS + 1);
		deps.taskWorkspaceCode.mockResolvedValue('ws');
		await sync.run();
		expect(deps.postComment).toHaveBeenCalledTimes(1);
		expect(marked).toEqual(['c_1#1']);
		warn.mockRestore();
	});

	it('posts exactly once for concurrent triggers and then marks', async () => {
		const { deps, marked, sync } = setup();
		await Promise.all([sync.run(), sync.run()]);
		expect(deps.postComment).toHaveBeenCalledTimes(1);
		expect(deps.postComment).toHaveBeenCalledWith(
			42,
			'answer [decision c_1#1]',
		);
		expect(deps.invoke).toHaveBeenCalledWith('decisions_mark_task_comment', {
			cardId: 'c_1',
			seq: 1,
		});
		expect(marked).toEqual(['c_1#1']);
	});

	it('skips the post but marks when the marker is already present', async () => {
		const { deps, marked, sync } = setup();
		deps.fetchComments.mockResolvedValue([{ message: 'x [decision c_1#1] y' }]);
		await sync.run();
		expect(deps.postComment).not.toHaveBeenCalled();
		expect(marked).toEqual(['c_1#1']);
	});

	it('leaves the item pending on network errors and posts once on retry', async () => {
		const { deps, marked, sync } = setup();
		deps.postComment.mockRejectedValueOnce(new Error('Network Error'));
		await sync.run();
		expect(marked).toEqual([]);
		await sync.run();
		expect(deps.postComment).toHaveBeenCalledTimes(2);
		expect(marked).toEqual(['c_1#1']);
	});

	it('leaves the item pending on 5xx', async () => {
		const { deps, marked, sync } = setup();
		deps.postComment.mockRejectedValueOnce(httpError(502));
		await sync.run();
		expect(marked).toEqual([]);
		await sync.run();
		expect(marked).toEqual(['c_1#1']);
	});

	it('backs off on 404 without marking, then retries after the window', async () => {
		const { deps, marked, advance, sync } = setup();
		deps.fetchComments.mockRejectedValueOnce(httpError(404));
		await sync.run();
		expect(marked).toEqual([]);
		await sync.run();
		expect(deps.fetchComments).toHaveBeenCalledTimes(1);
		advance(BACKOFF_MS + 1);
		await sync.run();
		expect(deps.postComment).toHaveBeenCalledTimes(1);
		expect(marked).toEqual(['c_1#1']);
	});

	it('backs off on 403 from the post', async () => {
		const { deps, marked, sync } = setup();
		deps.postComment.mockRejectedValueOnce(httpError(403));
		await sync.run();
		await sync.run();
		expect(deps.postComment).toHaveBeenCalledTimes(1);
		expect(marked).toEqual([]);
	});

	it.each([400, 401, 410, 422])('backs off on %i', async (status) => {
		const { deps, marked, sync } = setup();
		deps.postComment.mockRejectedValueOnce(httpError(status));
		await sync.run();
		await sync.run();
		expect(deps.postComment).toHaveBeenCalledTimes(1);
		expect(marked).toEqual([]);
	});

	it.each([408, 429])('retries on the next tick after %i', async (status) => {
		const { deps, marked, sync } = setup();
		deps.postComment.mockRejectedValueOnce(httpError(status));
		await sync.run();
		await sync.run();
		expect(deps.postComment).toHaveBeenCalledTimes(2);
		expect(marked).toEqual(['c_1#1']);
	});

	it('skips everything when the API is not production', async () => {
		const { deps, marked } = setup();
		const sync = createDecisionTaskCommentSync({ ...deps, origin: null });
		await sync.run();
		expect(deps.fetchComments).not.toHaveBeenCalled();
		expect(marked).toEqual([]);
	});

	it('skips items from another origin without marking', async () => {
		const { deps, marked, sync } = setup([
			item({ origin: 'https://other.example' }),
		]);
		await sync.run();
		expect(deps.fetchComments).not.toHaveBeenCalled();
		expect(marked).toEqual([]);
	});

	it('keeps going after one item fails', async () => {
		const { deps, marked, sync } = setup([
			item(),
			item({ card_id: 'c_2', task_id: 43, body: 'b [decision c_2#1]' }),
		]);
		deps.fetchComments.mockRejectedValueOnce(new Error('Network Error'));
		await sync.run();
		expect(marked).toEqual(['c_2#1']);
	});

	it('debounces change triggers into a single run', async () => {
		jest.useFakeTimers();
		try {
			const { deps, sync } = setup();
			sync.trigger();
			sync.trigger();
			sync.trigger();
			await jest.advanceTimersByTimeAsync(2100);
			expect(deps.postComment).toHaveBeenCalledTimes(1);
			sync.stop();
		} finally {
			jest.useRealTimers();
		}
	});
});
