jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: {
		get: jest.fn(),
		post: jest.fn(),
		patch: jest.fn(),
		delete: jest.fn(),
	},
}));

import axios from '@/plugins/axios';
import {
	approveDeviceRequest,
	denyDeviceRequest,
	getDeviceRequest,
	listCliDevices,
	revokeCliDevice,
	updateCliDeviceScopes,
} from '../devices';

const http = axios as unknown as Record<string, jest.Mock>;

beforeEach(() => jest.clearAllMocks());

describe('device request calls', () => {
	it('unwraps the data envelope on lookup', async () => {
		http.get.mockResolvedValue({ data: { data: { client_name: 'tmgr-cli' } } });
		await expect(getDeviceRequest('ABCD-2345')).resolves.toEqual({
			client_name: 'tmgr-cli',
		});
		expect(http.get).toHaveBeenCalledWith('/auth/device/requests/ABCD-2345');
	});

	it('posts approve with scopes and deny without a body', async () => {
		http.post.mockResolvedValue({});
		await approveDeviceRequest('ABCD-2345', ['tasks:read']);
		expect(http.post).toHaveBeenCalledWith(
			'/auth/device/requests/ABCD-2345/approve',
			{ scopes: ['tasks:read'] },
		);
		await denyDeviceRequest('ABCD-2345');
		expect(http.post).toHaveBeenLastCalledWith(
			'/auth/device/requests/ABCD-2345/deny',
		);
	});
});

describe('signed-in device calls', () => {
	it('lists, patches and deletes', async () => {
		http.get.mockResolvedValue({ data: { data: [{ id: 'd1' }] } });
		await expect(listCliDevices()).resolves.toEqual([{ id: 'd1' }]);
		expect(http.get).toHaveBeenCalledWith('/auth/devices');

		http.patch.mockResolvedValue({});
		await updateCliDeviceScopes('d1', ['tasks:read']);
		expect(http.patch).toHaveBeenCalledWith('/auth/devices/d1', {
			scopes: ['tasks:read'],
		});

		http.delete.mockResolvedValue({});
		await revokeCliDevice('d1');
		expect(http.delete).toHaveBeenCalledWith('/auth/devices/d1');
	});
});
