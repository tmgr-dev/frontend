import $axios from 'src/plugins/axios';
import store from 'src/store';

export interface Login {
	email: string;
	password: string;
}

export interface Register {
	name: string;
	email: string;
	password: string;
	password_confirmation: string;
}

export const login = async (payload: Login) => {
	const {
		data: { data: token },
	} = await $axios.post('auth/login', payload);
	store.commit('token', token);
};

export const register = async (payload: Register) => {
	const {
		data: { data: token },
	} = await $axios.post('auth/register', payload);
	store.commit('token', token);
};
