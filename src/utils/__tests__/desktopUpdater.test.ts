const check = jest.fn();
const relaunch = jest.fn();

jest.mock('@tauri-apps/plugin-updater', () => ({ check }));
jest.mock('@tauri-apps/plugin-process', () => ({ relaunch }));

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
	relaunch.mockReset();
});

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

it('installs the downloaded update and relaunches', async () => {
	const update = fakeUpdate('0.1.1');
	check.mockResolvedValue(update);
	const { checkForUpdate, installUpdate, updateState } = load();
	await checkForUpdate();

	await installUpdate();

	expect(update.install).toHaveBeenCalledTimes(1);
	expect(relaunch).toHaveBeenCalledTimes(1);
	expect(updateState.value.status).toBe('installing');
});

it('returns to ready when installation fails', async () => {
	const update = fakeUpdate('0.1.1');
	update.install.mockRejectedValue(new Error('disk full'));
	check.mockResolvedValue(update);
	const { checkForUpdate, installUpdate, updateState } = load();
	await checkForUpdate();

	await installUpdate();

	expect(relaunch).not.toHaveBeenCalled();
	expect(updateState.value.status).toBe('ready');
});
