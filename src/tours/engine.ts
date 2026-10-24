import { STARTERS_EXPECTED_KEY } from '@/onboarding/starters';
import type { Ref } from 'vue';
import { computed, ref } from 'vue';
import { resolveAnchor as defaultResolveAnchor } from './anchors';
import type { OnboardingApi } from './api';
import { getTour, TOURS } from './catalog';
import type { TourBus } from './events';
import type { KeycapEnv } from './keycaps';
import { DEFAULT_KEYCAP_ENV, hasTemplate, resolveBodyParts } from './keycaps';
import type { TourLease } from './lease';
import type { OverlayAdapter } from './overlay/OverlayAdapter';
import type { RunnerView } from './runner';
import { TourRunner } from './runner';
import type { ToursStore } from './store';
import { createToursStore } from './store';
import { tourTelemetry } from './telemetry';
import type { Triggers } from './triggers';
import { createTriggers, SESSION_KEY } from './triggers';
import type {
	Platform,
	StepDef,
	TipsMode,
	TourDef,
	TourId,
	TourRequires,
	TourState,
} from './types';
import type { ActiveTour, HubItem, ResumePill, ToursApi } from './useTours';

export interface OverlayRegistration {
	adapter: OverlayAdapter;
	mount?: (host: HTMLElement) => () => void;
}

export interface ToursHostDeps {
	api: OnboardingApi;
	bus: TourBus;
	lease: TourLease;
	storage: {
		getItem(k: string): string | null;
		setItem(k: string, v: string): void;
	};
	session: {
		getItem(k: string): string | null;
		setItem(k: string, v: string): void;
	};
	now(): number;
	platform(): Platform;
	mobile(): boolean;
	account(): 'cloud' | 'local';
	isOwner(): boolean;
	modulesKnown(): boolean;
	isModuleEnabled(key: string): boolean;
	currentRouteName(): string | null;
	currentRouteQuery?(): Record<string, unknown>;
	navigate(route: NonNullable<StepDef['route']>): void;
	isBusy(): boolean;
	overlay(): OverlayRegistration | null;
	setTimer(fn: () => void, ms: number): () => void;
	facts?(): Record<string, unknown>;
	capability?(name: NonNullable<TourRequires['capability']>): boolean;
	keycaps?(): KeycapEnv | null;
	hasPersonas?(): Promise<boolean>;
	resolveAnchor?: typeof defaultResolveAnchor;
}

export interface ToursEngine {
	api: ToursApi;
	runner: TourRunner;
	store: ToursStore;
	triggers: Triggers;
	init(): Promise<void>;
	signedOut(): void;
	routeEntered(name: string | null): void;
	modulesChanged(): void;
	rerender(): void;
	dispose(): void;
}

const LEASE_RENEW_MS = 10_000;
const OFFER_POLL_MS = 2_000;
const WATCH_ANCHOR_MS = 10 * 60_000;

export function createTours(deps: ToursHostDeps): ToursEngine {
	const active = ref<ActiveTour | null>(null) as Ref<ActiveTour | null>;
	const resumePill = ref<ResumePill | null>(null) as Ref<ResumePill | null>;
	const hostReady = ref(!!deps.overlay());
	const personaKnown = ref<boolean | null>(null);
	let personaFetch: Promise<void> | null = null;
	let lastView: RunnerView | null = null;
	let shownKey = '';
	let cancelRenew: (() => void) | null = null;
	let sequence: { current: TourId; rest: TourId[]; trigger: string } | null =
		null;

	const store = createToursStore({
		api: deps.api,
		platform: () => deps.platform(),
		onSaved: (id, state) =>
			deps.lease.broadcast({ type: 'tour_state', id, state }),
	});

	deps.lease.onMessage((message) => {
		if (message.type !== 'tour_state' || typeof message.id !== 'string') return;
		store.applyRemote(message.id, message.state as TourState);
	});

	const offPersonaCreated = deps.bus.on('persona.created', () => {
		personaKnown.value = true;
	});

	let personaEpoch = 0;
	let personaDeferred = false;

	const resetPersonas = () => {
		personaEpoch++;
		personaDeferred =
			personaDeferred || personaKnown.value !== null || personaFetch !== null;
		personaKnown.value = null;
		personaFetch = null;
	};

	const personasAskable = () =>
		!!deps.hasPersonas &&
		deps.modulesKnown() &&
		deps.isModuleEnabled('personas');

	const ensurePersonas = (): Promise<void> => {
		if (personaKnown.value === true || !deps.hasPersonas)
			return Promise.resolve();
		if (!personasAskable()) {
			personaDeferred = true;
			return Promise.resolve();
		}
		personaDeferred = false;
		if (!personaFetch) {
			const epoch = personaEpoch;
			const pending: Promise<void> = deps.hasPersonas!()
				.then(
					(has) => {
						if (epoch !== personaEpoch) return;
						if (has) personaKnown.value = true;
						else if (personaKnown.value === null) personaKnown.value = false;
					},
					() => {},
				)
				.finally(() => {
					if (personaFetch === pending) personaFetch = null;
				});
			personaFetch = pending;
		}
		return personaFetch;
	};

	const unavailableReason = (
		tour: TourDef,
	): HubItem['unavailableReason'] | undefined => {
		const req = tour.requires;
		if (req?.platforms && !req.platforms.includes(deps.platform()))
			return 'platform';
		if (req?.capability && !(deps.capability?.(req.capability) ?? false))
			return 'platform';
		if (req?.account && req.account !== 'any' && req.account !== deps.account())
			return 'account';
		if (req?.role === 'owner' && !deps.isOwner()) return 'role';
		if (
			deps.modulesKnown() &&
			(req?.modules ?? []).some((key) => !deps.isModuleEnabled(key))
		)
			return 'module_off';
		if (
			deps.modulesKnown() &&
			(req?.modulesOff ?? []).some((key) => deps.isModuleEnabled(key))
		)
			return 'module_on';
		if (req?.needs?.includes('persona') && deps.hasPersonas) {
			if (personaKnown.value === null) void ensurePersonas();
			else if (personaKnown.value === false) return 'needs_persona';
		}
		return undefined;
	};

	const hubItems = computed<HubItem[]>(() =>
		TOURS.map((tour) => {
			const reason = unavailableReason(tour);
			return {
				tour,
				state: store.state.value?.tours[tour.id] ?? null,
				available: !reason,
				...(reason ? { unavailableReason: reason } : {}),
			};
		}),
	);

	const stopRenewing = () => {
		cancelRenew?.();
		cancelRenew = null;
	};

	const keepLease = () => {
		stopRenewing();
		const tick = () => {
			cancelRenew = deps.setTimer(() => {
				if (runner.activeTourId) {
					if (
						typeof document === 'undefined' ||
						document.visibilityState !== 'hidden'
					)
						deps.lease.renew();
					tick();
				}
			}, LEASE_RENEW_MS);
		};
		tick();
	};

	const showView = (view: RunnerView | null) => {
		const registration = deps.overlay();
		if (!registration) return;
		if (!view) {
			shownKey = '';
			registration.adapter.hide();
			return;
		}
		const key = `${view.tour.id}/${view.step.id}/${view.mode}/${view.unavailable}/${view.alreadyDone}`;
		const same = key === shownKey && lastShownAnchor === view.anchorEl;
		shownKey = key;
		lastShownAnchor = view.anchorEl;
		if (same) registration.adapter.refresh();
		else
			registration.adapter.show({
				anchorEl: view.anchorEl,
				mode: view.mode,
				blocking:
					!view.unavailable && !(view.mode === 'passive' && !view.anchorEl),
				mount: registration.mount ?? (() => () => {}),
			});
	};
	let lastShownAnchor: Element | null = null;

	const runner: TourRunner = new TourRunner({
		getTour,
		stateOf: (id) => store.tourState(id),
		now: deps.now,
		mobile: deps.mobile,
		modulesKnown: deps.modulesKnown,
		isModuleEnabled: deps.isModuleEnabled,
		currentRouteName: deps.currentRouteName,
		currentRouteQuery: deps.currentRouteQuery,
		navigate: deps.navigate,
		resolveAnchor: (step, options) =>
			(deps.resolveAnchor ?? defaultResolveAnchor)(step, options),
		waitForAnchor: (step, options) =>
			(deps.resolveAnchor ?? defaultResolveAnchor)(step, {
				...options,
				timeoutMs: WATCH_ANCHOR_MS,
			}),
		facts: () => ({
			...(store.state.value?.facts ?? {}),
			...(deps.facts?.() ?? {}),
		}),
		save: (id, input) => void store.saveTour(id, input),
		bus: deps.bus,
		setTimer: deps.setTimer,
		eligible: (tour) => !unavailableReason(tour),
		render: (view) => {
			lastView = view;
			if (!view) {
				active.value = null;
			} else {
				const { anchorEl: _anchorEl, mode: _mode, ...rest } = view;
				active.value = hasTemplate(rest.step)
					? {
							...rest,
							bodyParts: resolveBodyParts(
								rest.step,
								deps.keycaps?.() ?? DEFAULT_KEYCAP_ENV,
							),
						}
					: rest;
			}
			showView(view);
		},
		onFinished: (id, reason) => {
			stopRenewing();
			deps.lease.release();
			const tour = getTour(id);
			resumePill.value =
				reason === 'navigated' && tour
					? { tourId: id, title: tour.title }
					: null;
			const chain = sequence;
			if (!chain || chain.current !== id) return;
			if (reason === 'completed') void runSequence(chain.rest, chain.trigger);
			else sequence = null;
		},
		onSkip: (id) => triggers.noteSkip(id),
	});

	const triggers = createTriggers({
		tours: () => TOURS,
		onboarding: () => store.state.value,
		tourState: (id) => store.tourState(id),
		now: deps.now,
		storage: deps.storage,
		session: deps.session,
		isBusy: deps.isBusy,
		hasActive: () => runner.activeTourId !== null,
		eligible: (tour) => !unavailableReason(tour),
		start: (id, trigger) => begin(id, trigger, false),
		setTimer: deps.setTimer,
	});

	async function begin(
		id: TourId,
		trigger: string,
		resume: boolean,
	): Promise<boolean> {
		if (getTour(id)?.requires?.needs?.includes('persona'))
			await ensurePersonas();
		if (!deps.lease.acquire()) return false;
		resumePill.value = null;
		const started = resume
			? await runner.resume(id)
			: await runner.start(id, trigger);
		if (started) keepLease();
		else deps.lease.release();
		return started;
	}

	const markAutoUsed = (used: boolean) => {
		try {
			deps.session.setItem(SESSION_KEY, JSON.stringify(used));
		} catch {
			return;
		}
	};

	async function runSequence(ids: TourId[], trigger: string): Promise<boolean> {
		for (let i = 0; i < ids.length; i++) {
			sequence = { current: ids[i], rest: ids.slice(i + 1), trigger };
			if (await begin(ids[i], trigger, false)) return true;
		}
		sequence = null;
		return false;
	}

	const offers = new Map<TourId, { result: Promise<boolean>; stop(): void }>();

	const offer = (
		id: TourId,
		trigger: string,
		options?: { retryMs?: number },
	): Promise<boolean> => {
		const tour = getTour(id);
		if (!tour) return Promise.resolve(false);
		const pending = offers.get(id);
		if (pending) return pending.result;
		const state = store.tourState(id);
		if (state && state.status !== 'skipped') return Promise.resolve(false);
		const deadline = deps.now() + (options?.retryMs ?? 0);
		let cancelTimer: (() => void) | null = null;
		let finish: (ok: boolean) => void = () => {};
		const result = new Promise<boolean>((resolve) => {
			finish = (ok) => {
				cancelTimer?.();
				cancelTimer = null;
				offers.delete(id);
				resolve(ok);
			};
		});
		offers.set(id, { result, stop: () => finish(false) });
		const attempt = () => {
			cancelTimer = null;
			if (triggers.canAutoOffer(tour)) {
				markAutoUsed(true);
				tourTelemetry().track('tour_offered', { tour: id, trigger });
				begin(id, trigger, false).then(
					(started) => {
						if (!started) markAutoUsed(false);
						finish(started);
					},
					() => {
						markAutoUsed(false);
						finish(false);
					},
				);
				return;
			}
			if (
				store.state.value?.tips_mode === 'auto' &&
				deps.isBusy() &&
				deps.now() < deadline
			) {
				cancelTimer = deps.setTimer(attempt, OFFER_POLL_MS);
				return;
			}
			finish(false);
		};
		attempt();
		return result;
	};

	const cancelOffers = () => {
		for (const pending of [...offers.values()]) pending.stop();
	};

	const api: ToursApi = {
		active,
		hubItems: hubItems as unknown as ToursApi['hubItems'],
		onboarding: store.state,
		hostReady,
		resumePill,
		start: async (id, trigger) => void (await begin(id, trigger, false)),
		resume: async (id) => void (await begin(id, 'resume', true)),
		pause: () => runner.pause(),
		next: () => void runner.next(),
		skipStep: () => void runner.skipStep(),
		skipTour: () => runner.skipTour(),
		takeMeThere: () => void runner.takeMeThere(),
		ensurePersonas,
		patchFacts: (facts) => {
			store.patchFacts(facts);
			runner.factsChanged();
		},
		dismissPill: () => {
			resumePill.value = null;
		},
		async setTipsMode(mode: TipsMode) {
			await store.setPreferences({ tips_mode: mode });
			tourTelemetry().track('tips_mode_changed', { mode });
		},
		async setHubHidden(hidden: boolean) {
			await store.setPreferences({ hub_hidden: hidden });
		},
		async startSequence(ids, trigger) {
			if (store.state.value?.tips_mode !== 'auto') return;
			markAutoUsed(true);
			if (!(await runSequence(ids, trigger))) markAutoUsed(false);
		},
		offer,
		async setIntent(intent, options) {
			const before = store.state.value;
			const previous = before
				? { intent: before.intent, invited: before.invited }
				: null;
			store.applyIntent(intent);
			let result: unknown;
			try {
				result = await deps.api.setIntent(intent, options);
			} catch (error) {
				const current = store.state.value;
				if (current && previous) Object.assign(current, previous);
				else if (current) current.intent = null;
				throw error;
			}
			const reply = (
				typeof result === 'object' && result !== null ? result : {}
			) as Record<string, unknown>;
			if (typeof reply.invited === 'boolean')
				store.applyIntent(intent, { invited: reply.invited });
			let expected =
				Number(reply.starter_tasks_created) > 0
					? Number(reply.starter_tasks_created)
					: 0;
			if (options?.seed === false && expected > 0) {
				try {
					await deps.api.removeStarterTasks();
					expected = 0;
				} catch (error) {
					console.error('removing starter tasks failed', error);
				}
			}
			try {
				deps.storage.setItem(
					STARTERS_EXPECTED_KEY,
					expected > 0 ? String(expected) : '',
				);
			} catch {}
			if (intent === 'skipped' || intent === 'agent') markAutoUsed(true);
		},
		async setChecklistHidden(hidden: boolean) {
			await store.setChecklistHidden(hidden);
		},
		async resetAll() {
			sequence = null;
			runner.abandon();
			await store.resetAll();
			triggers.reset();
			resumePill.value = null;
		},
	};

	return {
		api,
		runner,
		store,
		triggers,
		async init() {
			await store.load();
			const route = deps.currentRouteName();
			if (route) triggers.onRouteEntered(route);
		},
		signedOut() {
			sequence = null;
			cancelOffers();
			runner.abandon();
			triggers.dispose();
			store.clear();
			resumePill.value = null;
			resetPersonas();
			personaDeferred = false;
		},
		routeEntered(name) {
			runner.routeChanged(name);
			triggers.onRouteEntered(name);
		},
		modulesChanged() {
			if (!deps.modulesKnown()) resetPersonas();
			else if (personaKnown.value === null && personaDeferred)
				void ensurePersonas();
			runner.recheck();
		},
		rerender() {
			hostReady.value = !!deps.overlay();
			showView(lastView);
		},
		dispose() {
			offPersonaCreated();
			sequence = null;
			cancelOffers();
			runner.dispose();
			triggers.dispose();
			stopRenewing();
		},
	};
}
