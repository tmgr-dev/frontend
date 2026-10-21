const check = jest.fn();
const invoke = jest.fn();
const logError = jest.fn();
const logWarn = jest.fn();

jest.mock('@tauri-apps/plugin-updater', () => ({ check }));
jest.mock('@tauri-apps/plugin-log', () => ({
	error: (...args: unknown[]) => logError(...args),
	warn: (...args: unknown[]) => logWarn(...args),
}));
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
	logError.mockReset();
	logWarn.mockReset();
});

const deferred = <T>() => {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((r) => {
		resolve = r;
	});
	return { promise, resolve };
};

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
	expect(logWarn).toHaveBeenCalledWith(
		'[updater] background check failed: offline',
	);
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

	it('reports the error message, logs it and resets the state on failure', async () => {
		check.mockRejectedValue(
			'Could not fetch a valid release JSON from the remote',
		);
		const { checkForUpdateManually, updateState } = load();

		const outcome = await checkForUpdateManually();

		expect(outcome).toEqual({
			kind: 'error',
			message: 'Could not fetch a valid release JSON from the remote',
		});
		expect(updateState.value).toEqual({ status: 'idle', version: '' });
		expect(logError).toHaveBeenCalledWith(
			'[updater] manual check failed: Could not fetch a valid release JSON from the remote',
		);
	});

	it('passes a plain-string plugin rejection through as the message', async () => {
		check.mockRejectedValue('error sending request for url');
		const { checkForUpdateManually } = load();

		expect(await checkForUpdateManually()).toEqual({
			kind: 'error',
			message: 'error sending request for url',
		});
		expect(logError).toHaveBeenCalledWith(
			'[updater] manual check failed: error sending request for url',
		);
	});

	it('checks again after a failed check', async () => {
		check.mockRejectedValueOnce(new Error('timed out')).mockResolvedValue(null);
		const { checkForUpdateManually } = load();

		await checkForUpdateManually();
		const outcome = await checkForUpdateManually();

		expect(check).toHaveBeenCalledTimes(2);
		expect(outcome).toEqual({ kind: 'up-to-date' });
	});

	it('shares one check between concurrent clicks when up to date', async () => {
		check.mockResolvedValue(null);
		const { checkForUpdateManually, updateState } = load();

		const outcomes = await Promise.all([
			checkForUpdateManually(),
			checkForUpdateManually(),
		]);

		expect(check).toHaveBeenCalledTimes(1);
		expect(outcomes).toEqual([{ kind: 'up-to-date' }, { kind: 'up-to-date' }]);
		expect(updateState.value).toEqual({ status: 'idle', version: '' });
	});

	it('shares one check between concurrent clicks when an update exists', async () => {
		const update = fakeUpdate('0.1.1');
		check.mockResolvedValue(update);
		const { checkForUpdateManually } = load();

		const outcomes = await Promise.all([
			checkForUpdateManually(),
			checkForUpdateManually(),
		]);

		expect(check).toHaveBeenCalledTimes(1);
		expect(update.download).toHaveBeenCalledTimes(1);
		expect(outcomes).toEqual([
			{ kind: 'available', version: '0.1.1' },
			{ kind: 'available', version: '0.1.1' },
		]);
	});

	it('joins a background check that is already running', async () => {
		const found = deferred<ReturnType<typeof fakeUpdate> | null>();
		check.mockReturnValue(found.promise);
		const { checkForUpdate, checkForUpdateManually, updateState } = load();

		const auto = checkForUpdate();
		const manual = checkForUpdateManually();
		expect(updateState.value.status).toBe('checking');
		found.resolve(fakeUpdate('0.1.1'));
		await auto;

		expect(await manual).toEqual({ kind: 'available', version: '0.1.1' });
		expect(check).toHaveBeenCalledTimes(1);
		expect(updateState.value).toEqual({ status: 'ready', version: '0.1.1' });
	});

	it('gives a click that joined a failing background check the real error', async () => {
		let fail!: (reason: unknown) => void;
		check.mockReturnValue(
			new Promise((_, reject) => {
				fail = reject;
			}),
		);
		const { checkForUpdate, checkForUpdateManually, updateState } = load();

		const auto = checkForUpdate();
		const manual = checkForUpdateManually();
		fail(new Error('Could not fetch a valid release JSON from the remote'));
		await auto;

		expect(await manual).toEqual({
			kind: 'error',
			message: 'Could not fetch a valid release JSON from the remote',
		});
		expect(check).toHaveBeenCalledTimes(1);
		expect(updateState.value).toEqual({ status: 'idle', version: '' });
		expect(logError).toHaveBeenCalledWith(
			'[updater] manual check failed: Could not fetch a valid release JSON from the remote',
		);
	});

	it('gives every concurrent click the same error from one failed check', async () => {
		check.mockRejectedValue(new Error('signature verification failed'));
		const { checkForUpdateManually, updateState } = load();

		const outcomes = await Promise.all([
			checkForUpdateManually(),
			checkForUpdateManually(),
		]);

		expect(check).toHaveBeenCalledTimes(1);
		expect(outcomes).toEqual([
			{ kind: 'error', message: 'signature verification failed' },
			{ kind: 'error', message: 'signature verification failed' },
		]);
		expect(updateState.value).toEqual({ status: 'idle', version: '' });
	});

	it('checks afresh when clicked after a failed background check', async () => {
		check.mockRejectedValueOnce(new Error('offline')).mockResolvedValue(null);
		const { checkForUpdate, checkForUpdateManually } = load();

		await checkForUpdate();
		const outcome = await checkForUpdateManually();

		expect(check).toHaveBeenCalledTimes(2);
		expect(outcome).toEqual({ kind: 'up-to-date' });
	});

	it('shows checking for overlapping clicks, then ready once the update is in', async () => {
		const found = deferred<ReturnType<typeof fakeUpdate> | null>();
		check.mockReturnValue(found.promise);
		const { checkForUpdateManually, updateState } = load();

		const first = checkForUpdateManually();
		const second = checkForUpdateManually();
		expect(updateState.value.status).toBe('checking');
		found.resolve(fakeUpdate('0.1.1'));
		await Promise.all([first, second]);

		expect(updateState.value).toEqual({ status: 'ready', version: '0.1.1' });
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
