import axios from 'axios';
import store from '@/store';

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

		if (!config || !config.retry) {
			config.retry = 0;
		}

		if (error.response?.status === 401) {
			await store.dispatch('logout');
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
