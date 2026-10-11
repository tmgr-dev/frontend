import { onBeforeUnmount, onMounted, ref } from 'vue';

import { desktopPlatform } from '@/utils/desktop';
import {
	createNavHistory,
	navDirectionForKey,
	navDirectionForMouse,
	type NavDirection,
	type NavEntry,
} from '@/utils/navHistory';

interface NavRouter {
	currentRoute: { value: { fullPath: string } };
	afterEach: (hook: (to: { path: string }) => void) => unknown;
	push: (location: string) => Promise<unknown>;
}

interface NavStore {
	state: { currentTaskIdForModal: number | string | null };
	watch: (
		getter: () => unknown,
		cb: () => void,
		options?: { flush?: 'sync' },
	) => unknown;
	commit: (type: string, payload?: unknown) => void;
}

const history = createNavHistory();
const canBack = ref(false);
const canForward = ref(false);
let installed = false;
let applying = false;
let applyNavigation: (dir: NavDirection) => Promise<void> = async () => {};

const sync = () => {
	canBack.value = history.canBack();
	canForward.value = history.canForward();
};

export const installNavHistory = (
	router: NavRouter,
	store: NavStore,
): void => {
	if (installed) return;
	installed = true;

	const current = (): NavEntry => ({
		path: router.currentRoute.value.fullPath,
		taskId: store.state.currentTaskIdForModal ?? null,
	});
	const record = () => {
		if (current().path === '/app') return;
		history.push(current());
		sync();
	};
	let lastPosition: number | undefined;
	const recordRoute = (to: { path: string }) => {
		if (to.path === '/app') return;
		const position = globalThis.history?.state?.position;
		const replaced = position != null && position === lastPosition;
		lastPosition = position;
		if (replaced) history.replace(current());
		else history.push(current());
		sync();
	};

	router.afterEach(recordRoute);
	store.watch(() => store.state.currentTaskIdForModal, record, {
		flush: 'sync',
	});
	record();

	applyNavigation = async (dir) => {
		if (applying) return;
		const entry = dir === 'back' ? history.back() : history.forward();
		if (!entry) return;
		applying = true;
		try {
			if (router.currentRoute.value.fullPath !== entry.path) {
				await router.push(entry.path);
			}
			if (entry.taskId != null) {
				store.commit('setCurrentTaskIdForModal', entry.taskId);
			} else if (store.state.currentTaskIdForModal != null) {
				store.commit('closeTaskModal');
			}
		} finally {
			history.release();
			applying = false;
			sync();
		}
	};
};

export const useNavHistory = () => ({
	canBack,
	canForward,
	back: () => applyNavigation('back'),
	forward: () => applyNavigation('forward'),
});

const isEditable = (target: EventTarget | null): boolean => {
	const el = target as HTMLElement | null;
	if (!el || typeof el.closest !== 'function') return false;
	return (
		el.isContentEditable === true ||
		el.closest('input, textarea, select, [contenteditable=""], [contenteditable="true"]') !==
			null
	);
};

export const useNavHistoryInputs = () => {
	const nav = useNavHistory();
	const platform = desktopPlatform();
	const go = (dir: NavDirection | null) => {
		if (dir) void (dir === 'back' ? nav.back() : nav.forward());
	};
	const onKeyDown = (event: KeyboardEvent) => {
		const dir = navDirectionForKey(event, platform);
		if (!dir || isEditable(event.target)) return;
		event.preventDefault();
		go(dir);
	};
	const onMouseUp = (event: MouseEvent) => {
		const dir = navDirectionForMouse(event.button);
		if (!dir) return;
		event.preventDefault();
		go(dir);
	};
	const swallowMouseDown = (event: MouseEvent) => {
		if (navDirectionForMouse(event.button)) event.preventDefault();
	};

	let disposed = false;
	const unlisten: Array<() => void> = [];

	onMounted(async () => {
		window.addEventListener('keydown', onKeyDown);
		window.addEventListener('mouseup', onMouseUp);
		window.addEventListener('mousedown', swallowMouseDown);
		const { getCurrentWindow } = await import('@tauri-apps/api/window');
		const win = getCurrentWindow();
		for (const [event, dir] of [
			['menu://nav-back', 'back'],
			['menu://nav-forward', 'forward'],
		] as const) {
			const stop = await win.listen(event, () => go(dir));
			if (disposed) stop();
			else unlisten.push(stop);
		}
	});

	onBeforeUnmount(() => {
		disposed = true;
		window.removeEventListener('keydown', onKeyDown);
		window.removeEventListener('mouseup', onMouseUp);
		window.removeEventListener('mousedown', swallowMouseDown);
		unlisten.forEach((stop) => stop());
	});
};
