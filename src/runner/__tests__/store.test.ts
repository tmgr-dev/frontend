import * as api from '../api';
import {
	refreshRunnerStatus,
	resetRunnerStore,
	setRunnerEnabled,
	startRunnerStore,
	taskKey,
	useRunner,
} from '../store';

jest.mock('../api');
const mocked = api as jest.Mocked<typeof api>;

const session = (id: string, over: Record<string, unknown> = {}) =>
	({
		id,
		name: id,
		profile: 'claude-code',
		task: { workspaceId: -1000, taskId: 7 },
		state: { kind: 'working', sinceMs: 1, detail: null },
		createdMs: 100,
		...over,
	} as any);

let handler: (event: api.WatchEvent) => void;

beforeEach(() => {
	resetRunnerStore();
	jest.resetAllMocks();
	mocked.isRunnerSupported.mockReturnValue(true);
	mocked.runnerEnabled.mockResolvedValue(true);
	mocked.runnerStatus.mockResolvedValue({
		state: 'ready',
		version: '0.1.0',
		caps: [],
	});
	mocked.runnerSessions.mockResolvedValue([session('s_a')]);
	mocked.runnerWatch.mockImplementation(async (cb) => {
		handler = cb;
		return 5;
	});
	mocked.runnerUnwatch.mockResolvedValue();
	mocked.runnerSetEnabled.mockResolvedValue();
});

describe('runner store', () => {
	it('starts the watch only when supported and enabled', async () => {
		mocked.isRunnerSupported.mockReturnValue(false);
		await startRunnerStore();
		expect(mocked.runnerEnabled).not.toHaveBeenCalled();
		expect(mocked.runnerWatch).not.toHaveBeenCalled();

		mocked.isRunnerSupported.mockReturnValue(true);
		mocked.runnerEnabled.mockResolvedValue(false);
		await startRunnerStore();
		expect(mocked.runnerWatch).not.toHaveBeenCalled();
		expect(useRunner().enabled.value).toBe(false);
	});

	it('enable starts the watch and loads status and sessions', async () => {
		mocked.runnerEnabled.mockResolvedValue(false);
		await startRunnerStore();
		await setRunnerEnabled(true);
		expect(mocked.runnerSetEnabled).toHaveBeenCalledWith(true);
		expect(mocked.runnerWatch).toHaveBeenCalledTimes(1);
		const runner = useRunner();
		expect(runner.enabled.value).toBe(true);
		expect(runner.status.value?.state).toBe('ready');
		expect(runner.sessions.value).toHaveLength(1);
	});

	it('disable unwatches and clears everything', async () => {
		await startRunnerStore();
		await setRunnerEnabled(false);
		expect(mocked.runnerUnwatch).toHaveBeenCalledWith(5);
		const runner = useRunner();
		expect(runner.enabled.value).toBe(false);
		expect(runner.sessions.value).toEqual([]);
		expect(runner.status.value).toBeNull();
	});

	it('does not start twice', async () => {
		await startRunnerStore();
		await startRunnerStore();
		expect(mocked.runnerWatch).toHaveBeenCalledTimes(1);
	});

	it('maps the latest session to its task', async () => {
		await startRunnerStore();
		handler({
			type: 'sessions',
			sessions: [
				session('s_old', { createdMs: 10 }),
				session('s_new', { createdMs: 20 }),
				session('s_other', { task: { workspaceId: -1000, taskId: 8 } }),
				session('s_none', { task: null }),
				session('s_absent', { task: undefined }),
			],
		});
		const runner = useRunner();
		expect(runner.sessionForTask(-1000, 7)?.id).toBe('s_new');
		expect(runner.sessionForTask(-1000, 8)?.id).toBe('s_other');
		expect(runner.sessionForTask(-1000, 9)).toBeNull();
		expect(taskKey(-1000, 7)).toBe('-1000:7');
		expect(runner.sessionsForTask(-1000, 7).map((s) => s.id)).toEqual([
			's_new',
			's_old',
		]);
	});

	it('applies state events', async () => {
		await startRunnerStore();
		handler({
			type: 'state',
			session: 's_a',
			state: { kind: 'blocked', sinceMs: 2, detail: null },
		});
		expect(useRunner().sessions.value[0].state.kind).toBe('blocked');
	});

	it('drops any detail that is not an exit reason', async () => {
		await startRunnerStore();
		handler({
			type: 'state',
			session: 's_a',
			state: { kind: 'blocked', sinceMs: 2, detail: 'Delete prod?' as any },
		});
		expect(useRunner().sessions.value[0].state.detail).toBeNull();
		handler({
			type: 'sessions',
			sessions: [
				session('s_b', {
					state: { kind: 'blocked', sinceMs: 2, detail: 'secret' },
				}),
			],
		});
		expect(JSON.stringify(useRunner().sessions.value)).not.toContain('secret');
	});

	it('marks the runner absent on down and clears sessions', async () => {
		await startRunnerStore();
		handler({ type: 'down', message: 'gone' });
		const runner = useRunner();
		expect(runner.status.value?.state).toBe('absent');
		expect(runner.sessions.value).toEqual([]);
	});

	it('recovers status when sessions arrive after down', async () => {
		await startRunnerStore();
		handler({ type: 'down', message: 'gone' });
		handler({ type: 'sessions', sessions: [session('s_a')] });
		await Promise.resolve();
		await Promise.resolve();
		expect(useRunner().status.value?.state).toBe('ready');
	});

	it('reconnects the watch once the runner is back after down', async () => {
		jest.useFakeTimers();
		try {
			await startRunnerStore();
			mocked.runnerStatus.mockResolvedValue({
				state: 'absent',
				caps: [],
				message: 'away',
			});
			handler({ type: 'down', message: 'gone' });
			expect(mocked.runnerUnwatch).toHaveBeenCalledWith(5);
			await jest.advanceTimersByTimeAsync(3000);
			expect(mocked.runnerWatch).toHaveBeenCalledTimes(1);
			mocked.runnerStatus.mockResolvedValue({
				state: 'ready',
				version: '0.1.0',
				caps: [],
			});
			await jest.advanceTimersByTimeAsync(3000);
			expect(mocked.runnerWatch).toHaveBeenCalledTimes(2);
			expect(useRunner().status.value?.state).toBe('ready');
			expect(useRunner().sessions.value).toHaveLength(1);
		} finally {
			resetRunnerStore();
			jest.useRealTimers();
		}
	});

	it('stops reconnecting when the runner is disabled', async () => {
		jest.useFakeTimers();
		try {
			await startRunnerStore();
			handler({ type: 'down', message: 'gone' });
			await setRunnerEnabled(false);
			await jest.advanceTimersByTimeAsync(10000);
			expect(mocked.runnerWatch).toHaveBeenCalledTimes(1);
		} finally {
			resetRunnerStore();
			jest.useRealTimers();
		}
	});

	it('refreshRunnerStatus reports a failed call as absent', async () => {
		await startRunnerStore();
		mocked.runnerStatus.mockRejectedValue(new Error('boom'));
		await refreshRunnerStatus();
		expect(useRunner().status.value?.state).toBe('absent');
	});

	it('unwatches if disabled while the watch was starting', async () => {
		mocked.runnerEnabled.mockResolvedValue(false);
		await startRunnerStore();
		let release: (id: number) => void = () => {};
		mocked.runnerWatch.mockImplementation(
			() => new Promise<number>((resolve) => (release = resolve)),
		);
		const enabling = setRunnerEnabled(true);
		await Promise.resolve();
		await Promise.resolve();
		await setRunnerEnabled(false);
		release(9);
		await enabling;
		expect(mocked.runnerUnwatch).toHaveBeenCalledWith(9);
	});
});
