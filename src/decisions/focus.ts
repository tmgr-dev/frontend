const CARD_ID = /^c_[0-9a-f]{32}$/;

export const focusIdFrom = (value: unknown): string | null =>
	typeof value === 'string' && CARD_ID.test(value) ? value : null;

export const focusIdFromSearch = (search: string): string | null =>
	focusIdFrom(new URLSearchParams(search).get('card'));

export const focusIdFromPayload = (payload: unknown): string | null =>
	focusIdFrom((payload as { id?: unknown } | null)?.id);

export const resolveFocus = (
	cards: readonly { id: string }[],
	id: string | null,
): string | null => (id && cards.some((c) => c.id === id) ? id : null);
