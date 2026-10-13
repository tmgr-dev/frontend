import * as api from '../api';
import { runButtonModel, startRunForTask } from '../runButton';

jest.mock('../api');
const mocked = api as jest.Mocked<typeof api>;

const ready = { state: 'ready', caps: [] } as any;
const base = {
	supported: true,
	enabled: true,
	status: ready,
	workspaceId: -1000,
	hasSession: false,
};

describe('runButtonModel', () => {
	it('is hidden when unsupported or the flag is off', () => {
		expect(runButtonModel({ ...base, supported: false }).visible).toBe(false);
		expect(runButtonModel({ ...base, enabled: false }).visible).toBe(false);
	});

	it('offers a run on a local workspace', () => {
		expect(runButtonModel(base)).toMatchObject({
			visible: true,
			mode: 'run',
			disabled: false,
			tooltip: null,
		});
	});

	it('is disabled with an explanation for cloud workspaces', () => {
		expect(runButtonModel({ ...base, workspaceId: 12 })).toMatchObject({
			visible: true,
			disabled: true,
			tooltip: 'Agent runs work only with local workspaces for now.',
		});
	});

	it('is disabled when the runner is absent, incompatible or unknown', () => {
		for (const status of [
			{ state: 'absent', caps: [] },
			{ state: 'incompatible', caps: [] },
			null,
		]) {
			const model = runButtonModel({ ...base, status: status as any });
			expect(model.disabled).toBe(true);
		}
		expect(
			runButtonModel({ ...base, status: { state: 'absent', caps: [] } as any })
				.tooltip,
		).toBe('TMGR runner is not running. Start it with `tmgr runner start`.');
	});

	it('uses the runner message as the incompatible tooltip', () => {
		const status = {
			state: 'incompatible',
			caps: [],
			message: 'Update the TMGR runner',
		} as any;
		expect(runButtonModel({ ...base, status }).tooltip).toBe(
			'Update the TMGR runner',
		);
		expect(
			runButtonModel({
				...base,
				status: { state: 'incompatible', caps: [] } as any,
			}).tooltip,
		).toBe('Update the TMGR runner');
	});

	it('switches to Open terminal when a session exists', () => {
		expect(runButtonModel({ ...base, hasSession: true })).toMatchObject({
			mode: 'open',
			disabled: false,
		});
	});
});

describe('startRunForTask', () => {
	const args = {
		workspaceId: -1000,
		taskId: 7,
		profile: 'codex',
	};

	it('passes exactly the contract arguments', async () => {
		mocked.runnerStartRun.mockResolvedValue({ id: 's_a' } as any);
		await expect(startRunForTask(args)).resolves.toEqual({
			ok: true,
			session: { id: 's_a' },
		});
		expect(mocked.runnerStartRun).toHaveBeenCalledWith(args);
	});

	it.each(['cancelled', new Error('cancelled'), { message: 'cancelled' }])(
		'swallows cancellation (%p)',
		async (error) => {
			mocked.runnerStartRun.mockRejectedValue(error);
			await expect(startRunForTask(args)).resolves.toEqual({
				ok: false,
				cancelled: true,
			});
		},
	);

	it('reports other errors in readable form', async () => {
		mocked.runnerStartRun.mockRejectedValue('disabled');
		await expect(startRunForTask(args)).resolves.toEqual({
			ok: false,
			cancelled: false,
			error: 'Could not start the agent: disabled',
		});
		mocked.runnerStartRun.mockRejectedValue('busy');
		await expect(startRunForTask(args)).resolves.toEqual({
			ok: false,
			cancelled: false,
			error: 'The agent is busy, try again in a moment.',
		});
	});
});
