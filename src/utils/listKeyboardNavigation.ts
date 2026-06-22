export type NavDirection = 1 | -1;

export const nextFocusIndex = (
	current: number,
	length: number,
	direction: NavDirection,
): number => {
	if (length <= 0) return -1;

	if (current < 0 || current >= length) {
		return direction > 0 ? 0 : length - 1;
	}

	const next = current + direction;
	if (next < 0) return 0;
	if (next >= length) return length - 1;
	return next;
};

export const shouldIgnoreNavigationTarget = (
	target: EventTarget | null,
): boolean => {
	const el = target as (HTMLElement & { isContentEditable?: boolean }) | null;
	if (!el || typeof el.tagName !== 'string') return false;

	const tag = el.tagName.toLowerCase();
	if (tag === 'input' || tag === 'textarea' || tag === 'select') return true;
	if (el.isContentEditable) return true;

	if (typeof el.closest === 'function') {
		return Boolean(
			el.closest(
				'[contenteditable="true"], .ce-block, .codex-editor, [role="textbox"]',
			),
		);
	}

	return false;
};
