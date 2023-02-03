import $axios from 'src/plugins/axios';
import store from 'src/store';

export interface Login {
	email: string;
	password: string;
}

export const login = async (payload: Login) => {
	const {
		data: { data: token },
	} = await $axios.post('auth/login', payload);

	store.commit('token', token);
	$axios.defaults.headers.common.Authorization = `Bearer ${token.token}`;
};

export interface Register {
	name: string;
	email: string;
	password: string;
	password_confirmation: string;
}

export const register = async (payload: Register) => {
	const {
		data: { data: token },
	} = await $axios.post('auth/register', payload);

	store.commit('token', token);
	$axios.defaults.headers.common.Authorization = `Bearer ${token.token}`;
};

export const logout = async () => {
	await $axios.get('auth/logout');
};

export const resetPassword = async (payload: { email: string }) => {
	return await $axios.post('password/reset', payload);
};

export const setNewPassword = async (
	token: string,
	payload: { password: string; password_confirmation: string },
) => {
	return await $axios.post(`password/reset/${token}`, payload);
};
