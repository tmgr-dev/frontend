import axios from 'axios';
import store from '@/store';

const $axios = axios.create({
	baseURL: import.meta.env.VITE_API_BASE_URL,
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

$axios.interceptors.response.use(
	(response) => response,
	(error) => {
		if (error.response.status === 401) {
			store.dispatch('logout').then(() => {});
		}

		throw error;
	},
);

export default $axios;
