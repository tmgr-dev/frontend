jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), put: jest.fn() },
}));
jest.mock('@/store', () => ({
	__esModule: true,
	default: {
		state: { sessionGeneration: 1 },
		getters: { userSettingByKey: () => undefined, isLoggedIn: true },
		commit: jest.fn(),
	},
}));
jest.mock('@/utils/requestCache', () => ({
	__esModule: true,
	default: { getOrFetch: jest.fn() },
}));
jest.mock('@/utils/timezoneDetect', () => ({ detectUserTimeZone: jest.fn() }));

import store from '@/store';
import requestCache from '@/utils/requestCache';
import { detectUserTimeZone } from '@/utils/timezoneDetect';
import { getUser } from '../user';

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
