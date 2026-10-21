export const serverMessageFrom = (error: unknown, fallback: string): string => {
	const message = (
		error as { response?: { data?: { message?: unknown } } } | null
	)?.response?.data?.message;
	return typeof message === 'string' && message.trim() ? message : fallback;
};
