const SESSION_EXPIRED_KEY = 'session.expired';

/** Set only when a live session was killed by a failed refresh — never on a manual logout. */
export const markSessionExpired = (): void => {
	try {
		sessionStorage.setItem(SESSION_EXPIRED_KEY, '1');
	} catch {}
};

/** Reads and clears the flag, so the notice shows once. */
export const consumeSessionExpired = (): boolean => {
	try {
		if (sessionStorage.getItem(SESSION_EXPIRED_KEY) === null) {
			return false;
		}
		sessionStorage.removeItem(SESSION_EXPIRED_KEY);
		return true;
	} catch {
		return false;
	}
};
