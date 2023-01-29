import { getCurrentInstance } from 'vue';
import formError from './FormErrorsMixin';
export default function () {
	const { appContext: {config: {globalProperties: { $axios, $store, $router }}} } = getCurrentInstance();
	const { errors, message, tryCatchRequest } = formError();

	const setUser = async () => {
		$axios.defaults.headers = {
			Authorization: `Bearer ${$store.getters.token.token}`,
			'X-Requested-With': 'XMLHttpRequest',
		};
		const {
			data: { data },
		} = await $axios.get('user');

		$store.commit('user', data);
	};

	const login = async (loginData) => {
		await tryCatchRequest(async () => {
			const response = await $axios.post('auth/login', loginData);
			const {
				data: { data }
			} = response;
			$store.commit('token', data);
			await setUser();
			await $router.push({ name: 'CurrentTasksList' });

			await Promise.all([
				$store.dispatch('loadUserSettings'),
				$store.dispatch('loadStatuses'),
			]);
		});
	}

	return {
		errors,
		message,
		setUser,
		login
	}
};
