const check = jest.fn();
const invoke = jest.fn();

jest.mock('@tauri-apps/plugin-updater', () => ({ check }));
jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...args: unknown[]) => invoke(...args),
}));

const fakeUpdate = (version: string) => ({
	version,
	download: jest.fn().mockResolvedValue(undefined),
	install: jest.fn().mockResolvedValue(undefined),
});

const load = () => {
	let mod!: typeof import('../desktopUpdater');
	jest.isolateModules(() => {
		mod = require('../desktopUpdater');
	});
	return mod;
};

beforeEach(() => {
	check.mockReset();
	invoke.mockReset();
	invoke.mockResolvedValue(undefined);
});

const callOrder = (command: string) =>
	invoke.mock.invocationCallOrder[
		invoke.mock.calls.findIndex(([name]) => name === command)
	];

it('stays idle when no update is published', async () => {
	check.mockResolvedValue(null);
	const { checkForUpdate, updateState } = load();

	await checkForUpdate();

	expect(updateState.value).toEqual({ status: 'idle', version: '' });
});

it('downloads a found update in the background and marks it ready', async () => {
	const update = fakeUpdate('0.1.1');
	check.mockResolvedValue(update);
	const { checkForUpdate, updateState } = load();

	await checkForUpdate();

	expect(update.download).toHaveBeenCalledTimes(1);
	expect(updateState.value).toEqual({ status: 'ready', version: '0.1.1' });
});

it('does not download the same update twice', async () => {
	const update = fakeUpdate('0.1.1');
	check.mockResolvedValue(update);
	const { checkForUpdate } = load();

	await checkForUpdate();
	await checkForUpdate();

	expect(check).toHaveBeenCalledTimes(1);
});

it('swallows network errors and stays idle', async () => {
	check.mockRejectedValue(new Error('offline'));
	const { checkForUpdate, updateState } = load();

	await expect(checkForUpdate()).resolves.toBeUndefined();
	expect(updateState.value.status).toBe('idle');
});

it('marks the relaunch, installs the update, then relaunches', async () => {
	const update = fakeUpdate('0.1.1');
	check.mockResolvedValue(update);
	const { checkForUpdate, installUpdate, updateState } = load();
	await checkForUpdate();

	await installUpdate();

	expect(update.install).toHaveBeenCalledTimes(1);
	expect(invoke.mock.calls).toEqual([
		['mark_update_relaunch'],
		['relaunch_after_update'],
	]);
	const installOrder = update.install.mock.invocationCallOrder[0];
	expect(callOrder('mark_update_relaunch')).toBeLessThan(installOrder);
	expect(callOrder('relaunch_after_update')).toBeGreaterThan(installOrder);
	expect(updateState.value.status).toBe('installing');
});

it('still installs and relaunches when the relaunch marker fails', async () => {
	const update = fakeUpdate('0.1.1');
	check.mockResolvedValue(update);
	invoke.mockImplementation(async (command: string) => {
		if (command === 'mark_update_relaunch') throw new Error('read-only');
	});
	const { checkForUpdate, installUpdate } = load();
	await checkForUpdate();

	await installUpdate();

	expect(update.install).toHaveBeenCalledTimes(1);
	expect(invoke).toHaveBeenCalledWith('relaunch_after_update');
});

it('returns to ready when installation fails', async () => {
	const update = fakeUpdate('0.1.1');
	update.install.mockRejectedValue(new Error('disk full'));
	check.mockResolvedValue(update);
	const { checkForUpdate, installUpdate, updateState } = load();
	await checkForUpdate();

	await installUpdate();

	expect(invoke).not.toHaveBeenCalledWith('relaunch_after_update');
	expect(updateState.value.status).toBe('ready');
});

describe('checkForUpdateManually', () => {
	it('reports up to date and leaves the state idle', async () => {
		check.mockResolvedValue(null);
		const { checkForUpdateManually, updateState } = load();

		const outcome = await checkForUpdateManually();

		expect(outcome).toEqual({ kind: 'up-to-date' });
		expect(updateState.value).toEqual({ status: 'idle', version: '' });
	});

	it('downloads a found update and reports it available', async () => {
		const update = fakeUpdate('0.1.1');
		check.mockResolvedValue(update);
		const { checkForUpdateManually, updateState } = load();

		const outcome = await checkForUpdateManually();

		expect(update.download).toHaveBeenCalledTimes(1);
		expect(outcome).toEqual({ kind: 'available', version: '0.1.1' });
		expect(updateState.value).toEqual({ status: 'ready', version: '0.1.1' });
	});

	it('reports an error and resets the state on failure', async () => {
		check.mockRejectedValue(new Error('offline'));
		const { checkForUpdateManually, updateState } = load();

		const outcome = await checkForUpdateManually();

		expect(outcome).toEqual({ kind: 'error' });
		expect(updateState.value).toEqual({ status: 'idle', version: '' });
	});

	it('reports the already-downloaded update without checking again', async () => {
		const update = fakeUpdate('0.1.1');
		check.mockResolvedValue(update);
		const { checkForUpdate, checkForUpdateManually } = load();
		await checkForUpdate();

		const outcome = await checkForUpdateManually();

		expect(check).toHaveBeenCalledTimes(1);
		expect(outcome).toEqual({ kind: 'available', version: '0.1.1' });
	});
});
