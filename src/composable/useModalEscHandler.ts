import store from '@/store';

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

	const now = Date.now();
	if (now - lastCloseTimestamp < DEBOUNCE_MS) return;

	const openRadixDialog = document.querySelector(
		'[data-state="open"][role="dialog"]'
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
