export const hasTelegramLink = (
	user?: { telegram_id?: unknown; telegram_username?: unknown } | null,
): boolean => user?.telegram_id != null || user?.telegram_username != null;
