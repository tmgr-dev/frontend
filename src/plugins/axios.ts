import axios from 'axios';
import store from 'src/store';

const $axios = axios.create({
	baseURL: process.env.VUE_APP_API_BASE_URL,
	headers: {
		common: {
			Authorization: store.getters.token?.token
				? `Bearer ${store.getters.token.token}`
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
