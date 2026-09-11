import axios from 'axios';
import store from '@/store';
import { createTokenRefresher, isAuthUrl } from '@/utils/tokenRefresher';
import { parseStoredToken, TOKEN_STORAGE_KEY } from '@/utils/tokenSync';
import { isSocialCallbackPath, shouldReplayWithCurrentToken, wasSentWithCurrentToken } from '@/utils/sessionGuards';

const API_BASE_URL = import.meta.env.VITE_API_BASE_URL;
const API_TIMEOUT = 30000;

const $axios = axios.create({
	baseURL: API_BASE_URL,
	timeout: API_TIMEOUT,
	headers: {
		common: {
			// Authorization is attached per request by the interceptor below; reading
			// store here at module scope would join the store → actions → axios import cycle.
			'X-Requested-With': 'XMLHttpRequest',
			'Cache-Control': 'no-cache',
			Pragma: 'no-cache',
			Expires: '0',
		},
	},
});

// Every tab persists the envelope under the same key, so storage (not this
// tab's Vuex copy) is the source of truth for the refresh token: another tab
// may have rotated it while this one was throttled in the background.
const readStoredToken = () => {
	try {
		return parseStoredToken(localStorage.getItem(TOKEN_STORAGE_KEY)) ?? store.state.token ?? null;
	} catch {
		return store.state.token ?? null;
	}
};

// One refresh at a time across tabs; without Web Locks fall back to the
// per-tab single-flight inside the refresher.
const withCrossTabLock = <T>(fn: () => Promise<T>): Promise<T> =>
	typeof navigator !== 'undefined' && navigator.locks
		? navigator.locks.request('tmgr-token-refresh', fn)
		: fn();

// Bare axios (not $axios): the refresh call must bypass the 401 interceptor
// below, or a rejected refresh would recurse into itself.
const refreshAuthToken = createTokenRefresher({
	readStored: readStoredToken,
	currentAccessToken: () => store.state.token?.token,
	withLock: withCrossTabLock,
	post: (refreshToken) =>
		axios
			.post(
				'auth/refresh',
				{ refreshToken },
				{ baseURL: API_BASE_URL, timeout: API_TIMEOUT },
			)
			.then(({ data: { data } }) => data),
	onSuccess: (envelope) => store.commit('setToken', envelope),
});

const hardLogout = async () => {
	// The social OAuth callback page owns its session: it exchanges the code,
	// sets the fresh token and navigates itself. A 401 from a boot-time call
	// (guest or stale token) must neither wipe that token nor navigate away
	// mid-exchange — SocialiteProxy handles its own failure path.
	if (isSocialCallbackPath(window.location.pathname)) {
		return;
	}
	await store.dispatch('logout');
	const { default: router } = await import('@/router');
	router.push({ name: 'Login' }).catch(() => {});
};

$axios.interceptors.request.use(
	(config) => {
		const token = store.state.token?.token;
		if (token && config.headers) {
			config.headers.Authorization = `Bearer ${token}`;
		}
		return config;
	},
	(error) => Promise.reject(error)
);

$axios.interceptors.response.use(
	(response) => response,
	async (error) => {
		const config = error.config;

		// No config → nothing can be retried or replayed (request was
		// cancelled or failed before it was built).
		if (!config) {
			throw error;
		}
		if (!config.retry) {
			config.retry = 0;
		}

		if (error.response?.status === 401) {
			// The session rotated while this request was in flight (a social
			// login just set a fresh token, or another flow logged out): the
			// 401 belongs to the old session and must not touch the new one.
			if (!wasSentWithCurrentToken(config.headers?.Authorization, store.state.token?.token)) {
				// Another tab rotated the token while this request was in
				// flight (the storage listener already adopted it): replay
				// once with the current token, the request interceptor
				// re-attaches it.
				if (shouldReplayWithCurrentToken(config.headers?.Authorization, store.state.token?.token, !!config.__authRetried)) {
					config.__authRetried = true;
					return $axios(config);
				}
				throw error;
			}
			// A second 401 after a successful refresh+replay means the
			// new token is rejected too — give up.
			if (config.__authRetried) {
				await hardLogout();
				throw error;
			}
			// Failed login/register/refresh must not nuke the session.
			if (isAuthUrl(config.url)) {
				throw error;
			}
			const newAccessToken = await refreshAuthToken();
			if (newAccessToken) {
				config.__authRetried = true;
				return $axios(config);
			}
			await hardLogout();
			throw error;
		}

		if (
			error.response?.status >= 500 &&
			config.retry < 2 &&
			!config.__isRetry
		) {
			config.__isRetry = true;
			config.retry += 1;
			
			const delay = config.retry * 1000;
			await new Promise(resolve => setTimeout(resolve, delay));
			
			return $axios(config);
		}

		throw error;
	},
);

export default $axios;
