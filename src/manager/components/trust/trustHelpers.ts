export const groupFp = (fp: string, chars = 16): string =>
	(fp.slice(0, chars).match(/.{1,4}/g) ?? []).join(' ');

const FOCUSABLE = 'button:not([disabled]), input:not([disabled])';

export const focusInitial = (root: HTMLElement | undefined) => {
	const target =
		root?.querySelector<HTMLElement>('[data-autofocus]') ??
		root?.querySelector<HTMLElement>(FOCUSABLE);
	target?.focus();
};

export const dialogKeydown = (
	root: HTMLElement | undefined,
	e: KeyboardEvent,
	onEscape: () => void,
) => {
	if (e.key === 'Escape') {
		e.stopPropagation();
		onEscape();
		return;
	}
	if (e.key !== 'Tab' || !root) return;
	const items = Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE));
	if (!items.length) return;
	const first = items[0];
	const last = items[items.length - 1];
	if (e.shiftKey && document.activeElement === first) {
		e.preventDefault();
		last.focus();
	} else if (!e.shiftKey && document.activeElement === last) {
		e.preventDefault();
		first.focus();
	}
};

export const pairErrorKey = (raw: string) => {
	if (raw.includes('not set up')) return 'manager.trust.pair.errorNotSetUp';
	if (raw.includes('waiting for another device to approve'))
		return 'manager.trust.pair.errorNotApproved';
	return 'manager.trust.pair.errorGeneric';
};
