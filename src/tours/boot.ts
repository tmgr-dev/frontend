import {
	desktopPlatform,
	desktopWindowLabel,
	isDesktopApp,
	isMacLike,
	supportsLocalAccess,
	supportsPluginWindows,
	trayName,
} from '@/utils/desktop';
import type { AxiosInstance } from 'axios';
import type { Router } from 'vue-router';
import { onboardingApi } from './api';
import { installDesktopTriggers } from './desktopTriggers';
import { installTourInterceptor, tourBus } from './events';
import type { KeycapEnv } from './keycaps';
import { createBrowserLease } from './lease';
import type { Platform } from './types';
import { configureTours } from './useTours';

interface BootStore {
	state: any;
	getters: any;
	subscribe(
		fn: (mutation: { type: string; payload?: unknown }) => void,
	): () => void;
	watch<T>(
		getter: () => T,
		cb: (value: T) => void,
		options?: { immediate?: boolean },
	): () => void;
}

const SMALL_SCREEN_PX = 640;

const safeStorage = (kind: 'localStorage' | 'sessionStorage') => {
	const memory = new Map<string, string>();
	const fallback = {
		getItem: (k: string) => memory.get(k) ?? null,
		setItem: (k: string, v: string) => void memory.set(k, v),
		removeItem: (k: string) => void memory.delete(k),
	};
	try {
		const storage = window[kind];
		storage.getItem('tmgr:tours:probe');
		return storage;
	} catch {
		return fallback;
	}
};

export function isInputBusy(doc: Document = document): boolean {
	const el = doc.activeElement as HTMLElement | null;
	if (el && el !== doc.body) {
		const tag = el.tagName;
		if (
			tag === 'INPUT' ||
			tag === 'TEXTAREA' ||
			tag === 'SELECT' ||
			el.isContentEditable
		)
			return true;
	}
	return !!doc.querySelector(
		'[role="dialog"][data-state="open"], .sortable-chosen',
	);
}

export function installTours(
	axios: AxiosInstance,
	store: BootStore,
	router: Router,
): void {
	if (isDesktopApp() && desktopWindowLabel() !== 'main') return;

	installTourInterceptor(
		axios,
		(id) => store.getters.workspaceStatusById?.(id)?.type,
	);

	store.subscribe((mutation) => {
		if (mutation.type === 'setShowCreatingTaskModal')
			tourBus.emit('task.create_opened');
		if (
			(mutation.type === 'setAiPanelOpen' ||
				mutation.type === 'toggleAiPanel') &&
			store.state.aiPanelOpen
		) {
			tourBus.emit('ai.panel_opened');
		}
	});

	const mobile = () => window.innerWidth < SMALL_SCREEN_PX;
	const localOnly = () => !!store.getters.isLocalOnly;

	const lease = createBrowserLease();
	window.addEventListener('pagehide', () => lease.release());

	let shortcuts: typeof import('@/utils/desktopShortcuts') | null = null;
	if (isDesktopApp()) {
		void import('@/utils/desktopShortcuts').then((module) => {
			shortcuts = module;
		});
	}
	const keycaps = (): KeycapEnv | null =>
		isDesktopApp()
			? {
					mac: isMacLike(desktopPlatform()),
					tray: trayName(),
					shortcut: (action) =>
						shortcuts?.shortcutConfig.value[action as 'quickAdd'] ?? null,
					describe: (accelerator) =>
						shortcuts?.describeAccelerator(accelerator) ?? accelerator,
			  }
			: null;

	const engine = configureTours({
		api: onboardingApi(),
		bus: tourBus,
		lease,
		storage: safeStorage('localStorage'),
		session: safeStorage('sessionStorage'),
		now: Date.now,
		platform: (): Platform => {
			if (isDesktopApp()) return localOnly() ? 'desktop_local' : 'desktop';
			return mobile() ? 'mobile_web' : 'web';
		},
		mobile,
		account: () => (localOnly() ? 'local' : 'cloud'),
		isOwner: () => !!store.getters['featureToggles/canManageModules'],
		modulesKnown: () => !!store.getters['featureToggles/modulesKnown'],
		isModuleEnabled: (key) =>
			!!store.getters['featureToggles/isFeatureEnabled'](key),
		currentRouteName: () =>
			typeof router.currentRoute.value.name === 'string'
				? router.currentRoute.value.name
				: null,
		currentRouteQuery: () => router.currentRoute.value.query,
		navigate: (route) =>
			void router.push({
				name: route.name,
				...(route.params === 'currentWorkspace'
					? { params: { workspace_code: store.getters.currentWorkspace?.code } }
					: {}),
				...(route.query ? { query: route.query } : {}),
			}),
		isBusy: () => isInputBusy(),
		hasPersonas: async () => {
			if (localOnly()) return false;
			const { listPersonas } = await import('@/actions/tmgr/personas');
			return (await listPersonas(false, false)).length > 0;
		},
		capability: (name) =>
			name === 'localAccess' ? supportsLocalAccess() : supportsPluginWindows(),
		keycaps,
		setTimer: (fn, ms) => {
			const id = setTimeout(fn, ms);
			return () => clearTimeout(id);
		},
	});

	installDesktopTriggers(tourBus, engine.api, {
		isDesktop: isDesktopApp,
		windowLabel: desktopWindowLabel,
	});

	store.subscribe((mutation) => {
		if (mutation.type.startsWith('featureToggles/')) engine.modulesChanged();
	});
	router.afterEach((to) =>
		engine.routeEntered(typeof to.name === 'string' ? to.name : null),
	);
	store.watch(
		() => !!store.getters.canUseApp,
		(usable) => {
			if (usable) void engine.init();
			else engine.signedOut();
		},
		{ immediate: true },
	);
}
