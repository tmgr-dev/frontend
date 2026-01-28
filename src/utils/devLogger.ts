export const devLog = (...args: any[]) => {
	if (import.meta.env.DEV) {
		console.log(...args);
	}
};

export const devWarn = (...args: any[]) => {
	if (import.meta.env.DEV) {
		console.warn(...args);
	}
};

export const devError = (...args: any[]) => {
	if (import.meta.env.DEV) {
		console.error(...args);
	}
};

export const prodError = (...args: any[]) => {
	console.error(...args);
};
