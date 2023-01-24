import axios from 'axios';
import store from 'src/store';

axios.defaults.baseURL = store.getters.apiBaseUrl;

if (store.getters.token) {
	axios.defaults.headers.common = {
		Authorization: `Bearer ${store.getters.token.token}`,
		'X-Requested-With': 'XMLHttpRequest',
	};
	axios.get('user').then(({ data }) => {
		store.commit('user', data);
	});
}

export { axios };
