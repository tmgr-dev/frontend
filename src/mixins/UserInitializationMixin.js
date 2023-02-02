import { getCurrentInstance } from 'vue';
import useRequest from './useRequest';

export default function () {
	// @todo: There might be a more elegant way to declare plugins instead of declaring global properties
	const {
		appContext: {
			config: {
				globalProperties: { $axios, $store, $router },
			},
		},
	} = getCurrentInstance();
	const { errors, message, tryCatchRequest } = useRequest();

	const setUser = async () => {
		const {
			data: { data },
		} = await $axios.get('user');

		$store.commit('user', data);
	};

	const login = async (loginData) => {
		await tryCatchRequest(async () => {
			const response = await $axios.post('auth/login', loginData);
			const {
				data: { data },
			} = response;
			$store.commit('token', data);
			await setUser();
			await $router.push({ name: 'CurrentTasksList' });

			await Promise.all([
				$store.dispatch('loadUserSettings'),
				$store.dispatch('loadStatuses'),
			]);
		});
	};

	const register = async (registerData) => {
		await tryCatchRequest(async () => {
			const { data } = await $axios.post('auth/register', registerData);
			$store.commit('token', data.data);
			await setUser();
		});
	};

	return {
		errors,
		message,
		setUser,
		login,
		register,
	};
}
