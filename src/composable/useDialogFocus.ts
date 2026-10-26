import { getFocusable } from '@/tours/a11y';
import { Ref } from 'vue';
import { useStore } from 'vuex';

const FOREIGN_LAYER =
	'[role="dialog"],[role="alertdialog"],[role="menu"],[role="listbox"],[data-radix-popper-content-wrapper]';

// Menus, popovers and dialogs teleported out of a dialog own the keyboard while open; the other
// stack dialogs (marked data-dialog-root) are not such layers.
const isForeignLayer = (el: Element) => {
	const layer = el.closest(FOREIGN_LAYER);
	return !!layer && !layer.hasAttribute('data-dialog-root');
};

const focusables = (el: HTMLElement) =>
	getFocusable(el).filter((item) => item.getClientRects().length > 0);

export function useDialogFocus(
	id: string,
	root: Ref<HTMLElement | null>,
	fallbackLabel: string,
	fixedLabel = false,
) {
	const store = useStore();
	let opener: Element | null = null;
	let remembered = false;
	let pulledFrom: HTMLElement | null = null;
	let lastInside: HTMLElement | null = null;

	const isTop = () => {
		const stack = store.state.modalStack;
		return stack[stack.length - 1] === id;
	};

	function onKeydown(e: KeyboardEvent) {
		const el = root.value;
		if (e.key !== 'Tab' || !el || !isTop()) return;
		const active = document.activeElement;

		if (active && el.contains(active)) {
			const items = focusables(el);
			if (items.length === 0) {
				e.preventDefault();
				el.focus({ preventScroll: true });
				return;
			}
			const first = items[0];
			const last = items[items.length - 1];
			if (e.shiftKey && (active === first || active === el)) {
				e.preventDefault();
				last.focus({ preventScroll: true });
			} else if (!e.shiftKey && active === last) {
				e.preventDefault();
				first.focus({ preventScroll: true });
			}
			return;
		}

		if (active && isForeignLayer(active)) return;

		e.preventDefault();
		const items = focusables(el);
		const target = e.shiftKey ? items[items.length - 1] : items[0];
		(target ?? el).focus({ preventScroll: true });
	}

	// Focus handed back to the page behind (a closing menu returns it to its trigger) is pulled
	// back into the dialog.
	function onFocusin(e: FocusEvent) {
		const el = root.value;
		const target = e.target;
		if (!el || !(target instanceof HTMLElement) || !isTop()) return;
		if (el.contains(target)) {
			lastInside = target;
			const labelledBy = el.getAttribute('aria-labelledby');
			if (labelledBy && !document.getElementById(labelledBy)) label(el);
			return;
		}
		if (isForeignLayer(target)) return;
		pulledFrom = target;
		const back = lastInside?.isConnected ? lastInside : focusables(el)[0];
		(back ?? el).focus({ preventScroll: true });
	}

	function remember() {
		if (remembered) return;
		remembered = true;
		opener = document.activeElement;
	}

	function label(el: HTMLElement) {
		const heading = fixedLabel
			? null
			: el.querySelector<HTMLElement>('h1,h2,h3,[role="heading"]');
		if (heading) {
			heading.id ||= `${id}-title`;
			el.setAttribute('aria-labelledby', heading.id);
		} else {
			el.setAttribute('aria-label', fallbackLabel);
		}
	}

	function activate() {
		const el = root.value;
		if (!el) return;
		remember();
		el.setAttribute('data-dialog-root', '');
		label(el);
		if (!el.contains(document.activeElement)) {
			const target =
				el.querySelector<HTMLElement>('[data-autofocus],[autofocus]') ??
				focusables(el)[0];
			if (target) {
				target.focus({ preventScroll: true });
			} else {
				el.tabIndex = -1;
				el.focus({ preventScroll: true });
			}
		}
		document.addEventListener('keydown', onKeydown, true);
		document.addEventListener('focusin', onFocusin, true);
	}

	function deactivate() {
		document.removeEventListener('keydown', onKeydown, true);
		document.removeEventListener('focusin', onFocusin, true);
		// The opener can be a menu item that is gone by now; then the trigger the menu handed focus
		// back to (caught by onFocusin) is the next best place.
		const active = document.activeElement;
		const focusIsHere =
			!active || active === document.body || !!root.value?.contains(active);
		const back = [opener, pulledFrom].find(
			(el): el is HTMLElement =>
				el instanceof HTMLElement && el !== document.body && el.isConnected,
		);
		if (focusIsHere) back?.focus({ preventScroll: true });
		opener = pulledFrom = lastInside = null;
		remembered = false;
	}

	return { remember, activate, deactivate };
}
