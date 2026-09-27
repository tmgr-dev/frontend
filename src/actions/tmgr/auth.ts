import $axios from '@/plugins/axios';
import store from '@/store';

export interface LoginRequest {
	email: string;
	password: string;
}

export interface LoginToken {
	token: string;
	token_type?: string;
	expires_in?: number;
	expires_at?: string;
	refresh_token?: string;
	refresh_expires_in?: number;
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

export interface LoginGoogleRequest extends LoginWithCodeRequest {
	scope: string;
	ail: string;
	authuser: number;
	prompt: string;
}

const setAxiosHeaderBearerToken = ({
	data: { data: token },
}: LoginResponseWrapper): void => {
	store.commit('setToken', token);
	$axios.defaults.headers.common.Authorization = `Bearer ${token.token}`;
};

export const setTokenAndHeaders = (token: string): void => {
	const tokenData = { token: token }; // Wrap the string in an object to match LoginToken structure if needed by store
	store.commit('setToken', tokenData);
	$axios.defaults.headers.common.Authorization = `Bearer ${token}`;
};

export const login = (payload: LoginRequest): Promise<void> => {
	// No "remember me" checkbox by design: every session gets the long
	// (30d) refresh TTL instead of the 1-day no-remember fallback.
	return $axios
		.post('auth/login', { ...payload, rememberMe: true })
		.then(setAxiosHeaderBearerToken);
};

export const loginGithub = (payload: LoginWithCodeRequest): Promise<void> => {
	return $axios
		.post(`auth/login/github/redirect`, payload)
		.then(setAxiosHeaderBearerToken);
};

export const loginGoogle = (payload: LoginGoogleRequest): Promise<void> => {
	return $axios
		.post(`auth/login/google/redirect`, payload)
		.then(setAxiosHeaderBearerToken);
};

export const loginApple = (payload: LoginWithCodeRequest): Promise<void> => {
	return $axios
		.post(`auth/login/apple/accept`, payload)
		.then(setAxiosHeaderBearerToken);
};

export const loginTelegram = (payload: LoginWithCodeRequest): Promise<void> => {
	return $axios
		.post(`auth/login/telegram/redirect`, payload)
		.then(setAxiosHeaderBearerToken);
};

export interface Register {
	name: string;
	email: string;
	password: string;
	password_confirmation: string;
}

export const register = async (payload: Register) => {
	const response = await $axios.post('auth/register', payload);

	setAxiosHeaderBearerToken(response);
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

export interface DesktopRelayResult {
	code: string;
	state: string;
}

export const acceptDesktopLogin = (
	code: string,
	codeVerifier: string,
): Promise<void> =>
	$axios
		.post('auth/login/desktop/accept', { code, code_verifier: codeVerifier })
		.then(setAxiosHeaderBearerToken);

export const completeDesktopRelay = (
	provider: 'github' | 'google',
	code: string,
	tx: string,
): Promise<DesktopRelayResult> =>
	$axios
		.post(`auth/login/desktop/${provider}/complete`, { code, tx })
		.then((response) => response.data.data);

export const completeDesktopTelegramRelay = (
	tx: string,
	auth: Record<string, unknown>,
): Promise<DesktopRelayResult> =>
	$axios
		.post('auth/login/desktop/telegram/complete', { tx, auth })
		.then((response) => response.data.data);
