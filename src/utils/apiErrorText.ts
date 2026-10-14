const text = (value: unknown): string | undefined =>
	typeof value === 'string' && value.trim() ? value.trim() : undefined;

export function apiErrorText(error: unknown, fallback: string): string {
	const data = (error as { response?: { data?: unknown } } | null)?.response
		?.data;
	if (!data || typeof data !== 'object') {
		return fallback;
	}
	const body = data as { message?: unknown; error?: unknown };
	return text(body.message) ?? text(body.error) ?? fallback;
}
