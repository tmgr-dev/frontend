jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { put: jest.fn() },
}));
jest.mock('@/store', () => ({
	__esModule: true,
	default: {
		state: { user: { settings: [] as any[] } },
		getters: { userSettingByKey: () => undefined },
		commit: jest.fn(),
	},
}));

import $axios from '@/plugins/axios';
import store from '@/store';
import { persistThemeSettings } from '../user';

beforeEach(() => {
	jest.clearAllMocks();
	($axios.put as jest.Mock).mockResolvedValue({ data: { data: {} } });
});

test('never sends a settings item without a numeric id', async () => {
	(store.state as any).user.settings = [
		{ id: 6, key: 'theme', value: 'default' },
		{ id: 7, key: 'colorScheme', value: 'system' },
		{ key: 'timezone', value: 'Europe/Warsaw' },
		{ id: 9, key: 'other', value: 1 },
	];
	await persistThemeSettings('ocean', 'dark');
	expect($axios.put).toHaveBeenCalledWith('v2/user/settings', [
		{ id: 6, value: 'ocean' },
		{ id: 7, value: 'dark' },
		{ id: 9, value: 1 },
	]);
});
