import { api } from './client';
import { API_BASE_URL } from './config';
import { getToken, setToken } from './storage';

export type SocialProvider = 'google' | 'github' | 'apple';

// POST /auth/login -> { data: { token } }
export async function login(email: string, password: string): Promise<string> {
	const { data } = await api.post('auth/login', { email, password });
	const token = data?.data?.token as string | undefined;
	if (token) await setToken(token);
	if (!token) throw new Error('No token in login response');
	return token;
}

// POST /auth/register -> { data: { token } }
export async function register(
	name: string,
	email: string,
	password: string,
): Promise<string> {
	const { data } = await api.post('auth/register', {
		name,
		email,
		password,
		password_confirmation: password,
	});
	const token = data?.data?.token as string | undefined;
	if (token) await setToken(token);
	if (!token) throw new Error('No token in register response');
	return token;
}

export async function logout(): Promise<void> {
	try {
		await api.get('auth/logout');
	} catch {
		// ignore — clear locally regardless
	}
	await setToken(null);
}

// Current user (settings include current_workspace).
export async function getMe(): Promise<any> {
	const { data } = await api.get('user');
	return data?.data ?? data;
}

export async function isAuthenticated(): Promise<boolean> {
	return (await getToken()) != null;
}

// OAuth entry URL for a provider (opened in a browser session).
export function socialLoginUrl(provider: SocialProvider): string {
	return `${API_BASE_URL}auth/login/${provider}`;
}

export async function sendPasswordResetLink(email: string): Promise<void> {
	await api.post('password/reset', { email });
}
