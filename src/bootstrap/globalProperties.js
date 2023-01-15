import axios from 'axios';
import store from 'src/store';
import colorSchemes from 'src/colors/schemes';

axios.defaults.baseURL = store.getters.apiBaseUrl;

if (store.getters.token) {
	axios.defaults.headers = {
		Authorization: `Bearer ${store.getters.token.token}`,
		'X-Requested-With': 'XMLHttpRequest',
	};
	axios.get('user').then(({ data }) => {
		store.commit('user', data);
	});
}

const color = (colorKey) => colorSchemes[store.getters.colorScheme][colorKey];
document.querySelector('body').className = color('bgBody');

export { axios, color };
