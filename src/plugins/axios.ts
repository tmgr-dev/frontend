import axios from 'axios';
import store from '@/store';
import { refresh } from '@/actions/tmgr/auth';
import { singleFlight } from '@/plugins/tokenRefresh';

const refreshFlight = singleFlight<string | null>();

const $axios = axios.create({
	baseURL: import.meta.env.VITE_API_BASE_URL,
	timeout: 30000,
	headers: {
		common: {
			Authorization: store.state.token?.token
				? `Bearer ${store.state.token.token}`
				: '',
			'X-Requested-With': 'XMLHttpRequest',
			'Cache-Control': 'no-cache',
			Pragma: 'no-cache',
			Expires: '0',
		},
	},
});

$axios.interceptors.request.use(
	(config) => {
		const token = store.state.token?.token;
		if (token && config.headers) {
			config.headers.Authorization = `Bearer ${token}`;
		}
		return config;
	},
	(error) => Promise.reject(error)
);

$axios.interceptors.response.use(
	(response) => response,
	async (error) => {
		const config = error.config;
		if (!config) throw error;

		if (!config.retry) {
			config.retry = 0;
		}

		const status = error.response?.status;
		const isRefreshCall = (config.url || '').includes('auth/refresh');

		// Access token likely expired: try a single shared refresh, then retry once.
		if (status === 401 && !config.__authRetried && !isRefreshCall) {
			const storedRefresh = store.state.token?.refresh_token;
			if (!storedRefresh) {
				await store.dispatch('logout');
				throw error;
			}

			const newToken = await refreshFlight(async () => {
				try {
					await refresh(storedRefresh);
					return store.state.token?.token ?? null;
				} catch {
					return null;
				}
			});

			if (!newToken) {
				await store.dispatch('logout');
				throw error;
			}

			config.__authRetried = true;
			config.headers = config.headers || {};
			config.headers.Authorization = `Bearer ${newToken}`;
			return $axios(config);
		}

		if (status === 401) {
			// already retried after a refresh, or refresh endpoint itself 401'd
			await store.dispatch('logout');
			throw error;
		}

		if (status >= 500 && config.retry < 2 && !config.__isRetry) {
			config.__isRetry = true;
			config.retry += 1;
			const delay = config.retry * 1000;
			await new Promise((resolve) => setTimeout(resolve, delay));
			return $axios(config);
		}

		throw error;
	},
);

export default $axios;
