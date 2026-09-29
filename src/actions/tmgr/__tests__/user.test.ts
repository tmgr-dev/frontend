jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { put: jest.fn() },
}));
jest.mock('@/store', () => ({
	__esModule: true,
	default: {
		getters: {
			userSettingByKey: (key: string) =>
				key === 'current_workspace' ? { id: 5, value: 1 } : undefined,
			defaultWorkspaceId: 1,
		},
	},
}));

import $axios from '@/plugins/axios';
import store from '@/store';
import { updateUserSettingsV2 } from '../user';

beforeEach(() => {
	jest.clearAllMocks();
	(store.getters as any).userSettingByKey = (key: string) =>
		key === 'current_workspace' ? { id: 5, value: 1 } : undefined;
	(store.getters as any).defaultWorkspaceId = 1;
	($axios.put as jest.Mock).mockResolvedValue({ data: { data: {} } });
});

test('rewrites current_workspace back to the account default by default', async () => {
	await updateUserSettingsV2([
		{ id: 5, value: 2 },
		{ id: 6, value: 'dark' },
	]);
	expect($axios.put).toHaveBeenCalledWith('v2/user/settings', [
		{ id: 5, value: 1 },
		{ id: 6, value: 'dark' },
	]);
});

test('sends the requested value untouched when setDefaultWorkspace is true', async () => {
	await updateUserSettingsV2([{ id: 5, value: 2 }], { setDefaultWorkspace: true });
	expect($axios.put).toHaveBeenCalledWith('v2/user/settings', [{ id: 5, value: 2 }]);
});

test('is a no-op when the payload has no current_workspace entry', async () => {
	await updateUserSettingsV2([{ id: 6, value: 'dark' }]);
	expect($axios.put).toHaveBeenCalledWith('v2/user/settings', [{ id: 6, value: 'dark' }]);
});
