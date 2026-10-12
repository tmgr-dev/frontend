export const localStorageOf = (win: Window): Storage | null => {
	try {
		return win.localStorage;
	} catch {
		return null;
	}
};
