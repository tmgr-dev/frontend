import store from '@/store';
import { isTransientLayerOpen } from '@/tours/a11y';

type ModalCloseCallback = (options: { hasUnderlying: boolean }) => void;

interface ModalRegistration {
	id: string;
	close: ModalCloseCallback;
}

const modalRegistry: ModalRegistration[] = [];
let isListenerRegistered = false;
let lastCloseTimestamp = 0;
const DEBOUNCE_MS = 1000;

function handleGlobalEscape(e: KeyboardEvent) {
	if (e.key !== 'Escape') return;

	// A field marked data-esc-blur that holds a draft takes the first Esc itself (blurs, keeps the
	// text); the next Esc, with focus elsewhere, closes the modal.
	const target = e.target as HTMLTextAreaElement | null;
	if (target?.dataset?.escBlur !== undefined && target.value) return;

	// An open menu or listbox closes on this Esc itself; returning before the debounce lets the
	// next Esc close the modal right away.
	if (isTransientLayerOpen()) return;

	const now = Date.now();
	if (now - lastCloseTimestamp < DEBOUNCE_MS) return;

	const openRadixDialog = document.querySelector(
		'[data-state="open"][role="dialog"]:not(.tour-pop):not(.tour-sheet)',
	);
	if (openRadixDialog) {
		return;
	}

	const stack = store.state.modalStack;
	if (stack.length === 0 || modalRegistry.length === 0) {
		return;
	}

	e.preventDefault();
	e.stopImmediatePropagation();

	lastCloseTimestamp = now;

	const topmostId = stack[stack.length - 1];
	const topmost = modalRegistry.find((modal) => modal.id === topmostId);
	if (!topmost) {
		return;
	}

	const hasUnderlying = modalRegistry.length > 1;
	topmost.close({ hasUnderlying });
}

export function useModalEscHandler() {
	function registerModal(id: string, closeCallback: ModalCloseCallback) {
		const existing = modalRegistry.find((modal) => modal.id === id);
		if (existing) {
			existing.close = closeCallback;
		} else {
			modalRegistry.push({ id, close: closeCallback });
		}
		if (!isListenerRegistered) {
			document.addEventListener('keydown', handleGlobalEscape, true);
			isListenerRegistered = true;
		}
	}

	function unregisterModal(id: string) {
		const index = modalRegistry.findIndex((m) => m.id === id);
		if (index > -1) {
			modalRegistry.splice(index, 1);
		}
		if (modalRegistry.length === 0 && isListenerRegistered) {
			document.removeEventListener('keydown', handleGlobalEscape, true);
			isListenerRegistered = false;
		}
	}

	return { registerModal, unregisterModal };
}
