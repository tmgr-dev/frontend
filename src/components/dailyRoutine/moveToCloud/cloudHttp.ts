import axios from 'axios';
import { createCloudApi, type CloudApi } from './cloud';

/**
 * A server client that is not routed by the local-workspace interceptor on the app's axios, which
 * answers every routine and task call from this device while a local workspace is active.
 */
export const serverCloudApi = async (): Promise<CloudApi> => {
	const [{ default: $axios }, { default: store }] = await Promise.all([
		import('@/plugins/axios'),
		import('@/store'),
	]);
	const client = axios.create({
		baseURL: $axios.defaults.baseURL,
		timeout: 30000,
	});
	client.interceptors.request.use((config) => {
		const token = store.state.token?.token;
		if (token) config.headers.Authorization = `Bearer ${token}`;
		return config;
	});
	return createCloudApi(client, {
		refreshSession: async () => {
			await $axios.get('user');
		},
	});
};
