export interface AccountSnapshot {
	userId: number | null;
	signedIn: boolean;
}

export const USER_WAIT_MS = 15_000;
const USER_POLL_MS = 250;

/**
 * The store starts with an empty user and fills it after the session loads. Resolves with the user id
 * once known, or null when nobody is signed in or the user never loaded (so a slow start is not
 * mistaken for a different account).
 */
export const waitForUser = async (
	read: () => AccountSnapshot,
	sleep: (ms: number) => Promise<void>,
	timeoutMs = USER_WAIT_MS,
): Promise<{ known: boolean; userId: number | null }> => {
	for (let waited = 0; ; waited += USER_POLL_MS) {
		const { userId, signedIn } = read();
		if (userId != null) return { known: true, userId };
		if (!signedIn) return { known: true, userId: null };
		if (waited >= timeoutMs) return { known: false, userId: null };
		await sleep(USER_POLL_MS);
	}
};
