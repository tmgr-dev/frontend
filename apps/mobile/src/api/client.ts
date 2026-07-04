import axios, { AxiosError, InternalAxiosRequestConfig } from 'axios';
import { API_BASE_URL } from './config';
import { getToken, setToken } from './storage';

type RetryConfig = InternalAxiosRequestConfig & { __retry?: number };

// Mirrors the web axios setup: bearer token from storage, 401 -> logout,
// exponential retry (x2) on 5xx. There is no refresh endpoint on the backend,
// so a 401 clears the token and the app routes back to auth.
let onUnauthorized: (() => void) | null = null;
export function setUnauthorizedHandler(fn: (() => void) | null) {
	onUnauthorized = fn;
}

export const api = axios.create({
	baseURL: API_BASE_URL,
	timeout: 30000,
	headers: { Accept: 'application/json' },
});

api.interceptors.request.use(async (config) => {
	const token = await getToken();
	if (token && config.headers) {
		config.headers.Authorization = `Bearer ${token}`;
	}
	return config;
});

api.interceptors.response.use(
	(response) => response,
	async (error: AxiosError) => {
		const config = (error.config || {}) as RetryConfig;

		if (error.response?.status === 401) {
			await setToken(null);
			onUnauthorized?.();
			return Promise.reject(error);
		}

		const status = error.response?.status ?? 0;
		config.__retry = config.__retry ?? 0;
		if (status >= 500 && config.__retry < 2) {
			config.__retry += 1;
			await new Promise((r) => setTimeout(r, config.__retry! * 1000));
			return api(config);
		}

		return Promise.reject(error);
	},
);
