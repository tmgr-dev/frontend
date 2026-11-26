type LoadError = {
	message?: string;
	response?: { status?: number; data?: { message?: string } | string };
};

export const invitationLoadError = (e: LoadError, fallback: string): string => {
	const data = e.response?.data;
	const serverMessage = typeof data === 'object' ? data?.message : undefined;
	if (serverMessage) return serverMessage;
	if (e.response?.status) return `${fallback} (HTTP ${e.response.status})`;
	if (e.message) return `${fallback}: ${e.message}`;
	return fallback;
};
