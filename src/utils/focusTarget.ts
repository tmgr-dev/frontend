/**
 * Finds the field to put the cursor back into (TM-222). A template ref is either the element
 * itself or a component whose `$el` is the wrapper, so callers should not have to know which.
 * Written against shape rather than DOM classes so it can be reasoned about (and tested) without
 * a browser.
 */

export interface Focusable {
	focus: () => void;
}

const FIELD_TAGS = new Set(['INPUT', 'TEXTAREA']);

const isFocusable = (value: unknown): value is Focusable =>
	!!value && typeof (value as Focusable).focus === 'function';

export function resolveFocusTarget(ref: unknown): Focusable | null {
	const candidate = (ref as { $el?: unknown } | null | undefined)?.$el ?? ref;
	if (!candidate || typeof candidate !== 'object') return null;

	const tag = (candidate as { tagName?: string }).tagName;
	if (tag && FIELD_TAGS.has(tag) && isFocusable(candidate)) {
		return candidate as Focusable;
	}

	const query = (candidate as { querySelector?: (selector: string) => unknown }).querySelector;
	if (typeof query !== 'function') return null;

	const found = query.call(candidate, 'textarea, input');
	return isFocusable(found) ? found : null;
}

/** Puts the cursor back in the field behind `ref`, if there is one. */
export function focusField(ref: unknown): boolean {
	const target = resolveFocusTarget(ref);
	target?.focus();
	return !!target;
}
