jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), put: jest.fn(), patch: jest.fn() },
}));
jest.mock('@/store', () => ({
	__esModule: true,
	default: {
		state: jest.requireActual('vue').reactive({ sessionGeneration: 1 }),
		getters: { userSettingByKey: () => undefined, isLoggedIn: true },
		commit: jest.fn(),
	},
}));
jest.mock('@/utils/requestCache', () => ({
	__esModule: true,
	default: { getOrFetch: jest.fn() },
}));
jest.mock('@/utils/timezoneDetect', () => ({ detectUserTimeZone: jest.fn() }));

import axios from '@/plugins/axios';
import store from '@/store';
import requestCache from '@/utils/requestCache';
import { detectUserTimeZone } from '@/utils/timezoneDetect';
import { getUser, getUserSettings, patchUserSettings } from '../user';

const user = {
	id: 2,
	settings: [{ key: 'colorScheme', value: 'dark' }],
};

beforeEach(() => {
	jest.clearAllMocks();
	(store.state as any).sessionGeneration = 1;
});

test('a response from the current session is applied', async () => {
	(requestCache.getOrFetch as jest.Mock).mockResolvedValue(user);
	await expect(getUser()).resolves.toBe(user);
	expect(store.commit).toHaveBeenCalledWith('setUser', user);
	expect(detectUserTimeZone).toHaveBeenCalled();
});

test('a response that outlived its session has no effects', async () => {
	(requestCache.getOrFetch as jest.Mock).mockImplementation(async () => {
		(store.state as any).sessionGeneration = 2;
		return user;
	});
	await expect(getUser()).resolves.toBe(user);
	expect(store.commit).not.toHaveBeenCalled();
	expect(detectUserTimeZone).not.toHaveBeenCalled();
});

test('user settings from the current session are applied', async () => {
	(axios.get as jest.Mock).mockResolvedValue({
		data: { data: { settings: { a: 1 } } },
	});
	await getUserSettings();
	expect(store.commit).toHaveBeenCalledWith('setUserSettings', { a: 1 });
});

test('user settings that outlived their session are dropped', async () => {
	(axios.get as jest.Mock).mockImplementation(async () => {
		(store.state as any).sessionGeneration = 2;
		return { data: { data: { settings: { a: 1 } } } };
	});
	await getUserSettings();
	expect(store.commit).not.toHaveBeenCalled();
});

test('a pending private settings write is aborted when the session changes', async () => {
	const settings = { showTooltips: false };
	(axios.patch as jest.Mock).mockImplementation(
		async (_path, _body, { signal }) => {
			expect(signal.aborted).toBe(false);
			(store.state as any).sessionGeneration++;
			expect(signal.aborted).toBe(true);
			throw new Error('aborted');
		},
	);
	await expect(patchUserSettings(settings)).rejects.toThrow('aborted');
	expect(axios.patch).toHaveBeenCalledWith(
		'user/settings',
		settings,
		expect.objectContaining({ signal: expect.anything() }),
	);
});

test('a settings read started before a successful write cannot restore old preferences', async () => {
	let respond!: (response: unknown) => void;
	(axios.get as jest.Mock).mockImplementation(
		() =>
			new Promise((resolve) => {
				respond = resolve;
			}),
	);
	(axios.patch as jest.Mock).mockResolvedValue({ data: { data: {} } });
	const pending = getUserSettings();
	await patchUserSettings({ newView: 'Mine' });
	respond({ data: { data: { settings: {} } } });
	await pending;
	expect(store.commit).not.toHaveBeenCalled();
});
