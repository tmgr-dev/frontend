export type KeyAction =
	| { type: 'accept'; withNote: boolean }
	| { type: 'pick'; index: number; withNote: boolean }
	| { type: 'free' }
	| { type: 'defer' }
	| { type: 'chat' }
	| { type: 'dismiss' }
	| { type: 'prev' }
	| { type: 'next' }
	| { type: 'blur' };

export interface KeyInput {
	key: string;
	code?: string;
	shiftKey?: boolean;
	metaKey?: boolean;
	ctrlKey?: boolean;
	altKey?: boolean;
}

export interface KeyContext {
	typing: boolean;
	nativeEnter: boolean;
	optionCount: number;
	hasRecommended: boolean;
	canAnswer: boolean;
	canDismiss: boolean;
}

export const isSubmitKey = (event: KeyInput): boolean =>
	event.key === 'Enter' &&
	(Boolean(event.metaKey || event.ctrlKey) || !event.shiftKey);

export const mapKey = (
	event: KeyInput,
	context: KeyContext,
): KeyAction | null => {
	if (context.typing) {
		return event.key === 'Escape' ? { type: 'blur' } : null;
	}
	if (event.metaKey || event.ctrlKey || event.altKey) return null;
	const shift = Boolean(event.shiftKey);
	if (event.key === 'Enter') {
		if (context.nativeEnter) return null;
		return context.canAnswer && context.hasRecommended
			? { type: 'accept', withNote: shift }
			: null;
	}
	const digit = /^Digit([1-9])$/.exec(event.code ?? '');
	const number = digit
		? Number(digit[1])
		: /^[1-9]$/.test(event.key)
			? Number(event.key)
			: 0;
	if (number > 0) {
		return context.canAnswer && number <= context.optionCount
			? { type: 'pick', index: number - 1, withNote: shift }
			: null;
	}
	switch (event.key) {
		case 'ArrowDown':
			return { type: 'next' };
		case 'ArrowUp':
			return { type: 'prev' };
	}
	if (event.key.length !== 1) return null;
	switch (event.key.toLowerCase()) {
		case 't':
			return context.canAnswer ? { type: 'free' } : null;
		case 'd':
			return context.canAnswer ? { type: 'defer' } : null;
		case 'c':
			return { type: 'chat' };
		case 'x':
			return context.canDismiss ? { type: 'dismiss' } : null;
	}
	return null;
};

const NATIVE_ENTER = 'a[href], button, [role="button"], summary, select';

export interface KeyEventLike extends KeyInput {
	target: EventTarget | null;
	defaultPrevented: boolean;
}

export const resolveKey = (
	event: KeyEventLike,
	context: Omit<KeyContext, 'typing' | 'nativeEnter'>,
): KeyAction | null => {
	if (event.defaultPrevented) return null;
	const el = event.target as HTMLElement | null;
	const typing =
		!!el &&
		(el.tagName === 'TEXTAREA' ||
			el.tagName === 'INPUT' ||
			el.isContentEditable);
	const nativeEnter = !!el?.closest?.(NATIVE_ENTER);
	return mapKey(event, { ...context, typing, nativeEnter });
};
