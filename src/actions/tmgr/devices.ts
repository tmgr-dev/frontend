import $axios from '@/plugins/axios';

export interface DeviceRequest {
	client_name: string;
	host_label: string;
	requested_scopes: string[];
	created_at: string;
	expires_at: string;
}

export interface CliDevice {
	id: string;
	client_name: string;
	host_label: string;
	scopes: string[];
	created_at: string;
	last_used_at: string | null;
}

const requestPath = (userCode: string) =>
	`/auth/device/requests/${encodeURIComponent(userCode)}`;

export const getDeviceRequest = async (
	userCode: string,
): Promise<DeviceRequest> => {
	const {
		data: { data },
	} = await $axios.get(requestPath(userCode));
	return data;
};

export const approveDeviceRequest = async (
	userCode: string,
	scopes: string[],
): Promise<void> => {
	await $axios.post(`${requestPath(userCode)}/approve`, { scopes });
};

export const denyDeviceRequest = async (userCode: string): Promise<void> => {
	await $axios.post(`${requestPath(userCode)}/deny`);
};

export const listCliDevices = async (): Promise<CliDevice[]> => {
	const {
		data: { data },
	} = await $axios.get('/auth/devices');
	return data;
};

export const updateCliDeviceScopes = async (
	id: string,
	scopes: string[],
): Promise<CliDevice> => {
	const {
		data: { data },
	} = await $axios.patch(`/auth/devices/${encodeURIComponent(id)}`, {
		scopes,
	});
	return data;
};

export const revokeCliDevice = async (id: string): Promise<void> => {
	await $axios.delete(`/auth/devices/${encodeURIComponent(id)}`);
};
