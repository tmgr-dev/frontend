import type { TokenEnvelope } from '@/utils/tokenRefresher';
import {
	tokenFromStorageEvent,
	tokenSubject,
	type TokenSyncResult,
} from '@/utils/tokenSync';

export interface SessionSyncDeps {
	getToken: () => TokenEnvelope | null | undefined;
	getUserId: () => number | null | undefined;
	commitToken: (token: TokenEnvelope | null) => void;
	signOut: () => Promise<unknown> | unknown;
	clearInFlight: () => void;
	getUser: () => Promise<unknown>;
	disconnectRealtime: () => void;
}

/**
 * Mirror another tab's token change. A sign-out goes through the logout path
 * (which commits the null token itself); an adopted token reloads the user
 * unless it is a rotation for the account already loaded.
 */
export const syncSessionFromStorage = async (
	event: { key: string | null; newValue: string | null },
	deps: SessionSyncDeps,
): Promise<void> => {
	const result: TokenSyncResult = tokenFromStorageEvent(event, deps.getToken());
	if (!result.changed) {
		return;
	}
	const next = result.token;
	if (!next) {
		const signedOut = deps.getToken();
		await deps.signOut();
		if (deps.getToken() === signedOut) {
			deps.commitToken(null);
		}
		return;
	}
	deps.commitToken(next);
	const userId = deps.getUserId();
	if (userId != null && userId > 0 && tokenSubject(next) === userId) {
		return;
	}
	deps.clearInFlight();
	try {
		await deps.getUser();
	} catch {
		if (deps.getToken()?.token === next.token) {
			deps.disconnectRealtime();
		}
	}
};
