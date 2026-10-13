export const FOCUS_SOURCE_ATTR = 'data-focus-source';

// Libraries (Headless UI buttons, focus restore on close) call .focus() after a
// mouse click, and Chrome then matches :focus-visible. The global focus ring is
// gated on this attribute so a pointer interaction never draws it.
export const installFocusSource = (
	doc: Pick<Document, 'addEventListener' | 'documentElement'>,
): void => {
	const root = doc.documentElement;
	doc.addEventListener(
		'pointerdown',
		() => root.setAttribute(FOCUS_SOURCE_ATTR, 'pointer'),
		true,
	);
	doc.addEventListener(
		'keydown',
		() => root.removeAttribute(FOCUS_SOURCE_ATTR),
		true,
	);
};
