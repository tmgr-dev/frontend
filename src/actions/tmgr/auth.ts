import $axios from 'src/plugins/axios';
import store from 'src/store';

export interface LoginRequest {
	email: string;
	password: string;
}

export interface LoginToken {
	token: string;
}

export interface LoginResponse {
	data: LoginToken;
}

export interface LoginResponseWrapper {
	data: LoginResponse;
}

export interface LoginWithCodeRequest {
	code: string;
}

export interface LoginGoogleRequest extends LoginWithCodeRequest{
	scope: string;
	ail: string;
	authuser: number;
	prompt: string;
}

const setAxiosHeaderBearerToken = ({ data: { data: token } }: LoginResponseWrapper): void => {
	store.commit('setToken', token);
	$axios.defaults.headers.common.Authorization = `Bearer ${token.token}`;
};

export const login = (payload: LoginRequest): Promise<void> => {
	return $axios.post('auth/login', payload)
		.then(setAxiosHeaderBearerToken);
};

export const loginGithub = (payload: LoginWithCodeRequest): Promise<void> => {
	return $axios.post(`auth/login/github/redirect`, payload)
		.then(setAxiosHeaderBearerToken);
};

export const loginGoogle = (payload: LoginGoogleRequest): Promise<void> => {
	return $axios.post(`auth/login/google/redirect`, payload)
		.then(setAxiosHeaderBearerToken);
};

export const loginApple = (payload: LoginWithCodeRequest): Promise<void> => {
	return $axios.post(`auth/login/apple/accept`, payload)
		.then(setAxiosHeaderBearerToken);
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

	setAxiosHeaderBearerToken(token);
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
