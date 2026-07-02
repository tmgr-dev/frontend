import axios from 'axios';
import store from '@/store';
import { createTokenRefresher, isAuthUrl } from '@/utils/tokenRefresher';

const API_BASE_URL = import.meta.env.VITE_API_BASE_URL;
const API_TIMEOUT = 30000;

const $axios = axios.create({
	baseURL: API_BASE_URL,
	timeout: API_TIMEOUT,
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

// Bare axios (not $axios): the refresh call must bypass the 401 interceptor
// below, or a rejected refresh would recurse into itself.
const refreshAuthToken = createTokenRefresher({
	getRefreshToken: () => store.state.token?.refresh_token,
	post: (refreshToken) =>
		axios
			.post(
				'auth/refresh',
				{ refreshToken },
				{ baseURL: API_BASE_URL, timeout: API_TIMEOUT },
			)
			.then(({ data: { data } }) => data),
	onSuccess: (envelope) => store.commit('setToken', envelope),
});

const hardLogout = async () => {
	await store.dispatch('logout');
	const { default: router } = await import('@/router');
	router.push({ name: 'Login' }).catch(() => {});
};

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

		// No config → nothing can be retried or replayed (request was
		// cancelled or failed before it was built).
		if (!config) {
			throw error;
		}
		if (!config.retry) {
			config.retry = 0;
		}

		if (error.response?.status === 401) {
			// A second 401 after a successful refresh+replay means the
			// new token is rejected too — give up.
			if (config.__authRetried) {
				await hardLogout();
				throw error;
			}
			// Failed login/register/refresh must not nuke the session.
			if (isAuthUrl(config.url)) {
				throw error;
			}
			const newAccessToken = await refreshAuthToken();
			if (newAccessToken) {
				config.__authRetried = true;
				return $axios(config);
			}
			await hardLogout();
			throw error;
		}

		if (
			error.response?.status >= 500 &&
			config.retry < 2 &&
			!config.__isRetry
		) {
			config.__isRetry = true;
			config.retry += 1;
			
			const delay = config.retry * 1000;
			await new Promise(resolve => setTimeout(resolve, delay));
			
			return $axios(config);
		}

		throw error;
	},
);

export default $axios;
