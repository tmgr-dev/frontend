import { isDesktopApp } from '@/utils/desktop';
import {
	parseStoredToken,
	TOKEN_STORAGE_KEY,
	tokenSubject,
} from '@/utils/tokenSync';
import { invoke } from '@tauri-apps/api/core';

const readToken = () => {
	try {
		return parseStoredToken(localStorage.getItem(TOKEN_STORAGE_KEY));
	} catch {
		return null;
	}
};

/** Hands the cloud session to the chat relay in Rust; a no-op without a cloud token. */
export const syncRelayAuth = async (
	apiBase: string | undefined,
	userId?: number,
): Promise<void> => {
	if (!isDesktopApp() || !apiBase) return;
	const envelope = readToken();
	if (!envelope?.token) return;
	const id = userId ?? tokenSubject(envelope);
	if (!id) return;
	try {
		await invoke('manager_set_relay_auth', {
			token: envelope.token,
			userId: id,
			apiBase,
		});
	} catch {
		// Older shells lack the command; the error text could carry the token.
	}
};

export const clearRelayAuth = async (): Promise<void> => {
	if (!isDesktopApp()) return;
	try {
		await invoke('manager_clear_relay_auth');
	} catch {
		// Older shells lack the command.
	}
};

type Mutation = { type: string; payload?: unknown };
type RelayStore = {
	state: { token?: { token?: string } | null; user?: { id?: unknown } | null };
	subscribe: (fn: (mutation: Mutation) => void) => unknown;
};

const userIdOf = (user: { id?: unknown } | null | undefined) => {
	const id = Number(user?.id);
	return Number.isSafeInteger(id) && id > 0 ? id : undefined;
};

/** Main window only: mirrors login, token refresh, logout and account switch into Rust. */
export const installRelayAuthSync = (
	store: RelayStore,
	apiBase: string | undefined,
) => {
	let last: string | null = null;
	const push = () => {
		const token = store.state.token?.token;
		const userId = userIdOf(store.state.user);
		if (!token || !userId) {
			if (last === null) return;
			last = null;
			return void clearRelayAuth();
		}
		const key = `${userId}:${token}`;
		if (key === last) return;
		last = key;
		void syncRelayAuth(apiBase, userId);
	};
	push();
	store.subscribe((mutation) => {
		if (mutation.type === 'setToken' || mutation.type === 'setUser') push();
	});
};
