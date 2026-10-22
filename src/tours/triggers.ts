import { tourTelemetry } from './telemetry';
import type { OnboardingState, TourDef, TourId, TourState } from './types';

export const COOLDOWN_MS = 24 * 60 * 60 * 1000;
export const COOLDOWN_KEY = 'tmgr:tours:cooldown';
export const SKIPS_KEY = 'tmgr:tours:skips';
export const SESSION_KEY = 'tmgr:tours:auto';
export const ROUTE_SETTLE_MS = 600;

interface KeyValueStorage {
	getItem(key: string): string | null;
	setItem(key: string, value: string): void;
}

export interface TriggerDeps {
	tours: () => TourDef[];
	onboarding: () => OnboardingState | null;
	tourState: (id: TourId) => TourState | null;
	now: () => number;
	storage: KeyValueStorage;
	session: KeyValueStorage;
	isBusy: () => boolean;
	hasActive: () => boolean;
	eligible: (tour: TourDef) => boolean;
	start: (id: TourId, trigger: string) => Promise<unknown>;
	setTimer: (fn: () => void, ms: number) => () => void;
}

export interface Triggers {
	onRouteEntered(routeName: string | null): void;
	noteSkip(id: TourId): void;
	canAutoOffer(tour: TourDef): boolean;
	skipCount(id: TourId): number;
	reset(): void;
	dispose(): void;
}

const readJson = <T>(storage: KeyValueStorage, key: string, fallback: T): T => {
	try {
		const raw = storage.getItem(key);
		return raw ? (JSON.parse(raw) as T) : fallback;
	} catch {
		return fallback;
	}
};

const write = (storage: KeyValueStorage, key: string, value: unknown) => {
	try {
		storage.setItem(key, JSON.stringify(value));
	} catch {
		return;
	}
};

export function createTriggers(deps: TriggerDeps): Triggers {
	let pending: (() => void) | null = null;

	const skipCount = (id: TourId) => {
		const stored =
			readJson<Record<string, number>>(deps.storage, SKIPS_KEY, {})[id] ?? 0;
		const fromServer = deps.tourState(id)?.status === 'skipped' ? 1 : 0;
		return Math.max(stored, fromServer);
	};

	const coolingDown = () => {
		const at = readJson<number>(deps.storage, COOLDOWN_KEY, 0);
		return typeof at === 'number' && at > 0 && deps.now() - at < COOLDOWN_MS;
	};

	const autoUsed = () =>
		readJson<boolean>(deps.session, SESSION_KEY, false) === true;

	const offerableWhenIdle = (tour: TourDef) => {
		const onboarding = deps.onboarding();
		if (!onboarding || onboarding.tips_mode !== 'auto') return false;
		if (autoUsed() || coolingDown() || deps.hasActive()) return false;
		const state = deps.tourState(tour.id);
		if (state && !(state.status === 'skipped' && skipCount(tour.id) < 2))
			return false;
		if (skipCount(tour.id) >= 2) return false;
		return deps.eligible(tour);
	};

	const canAutoOffer = (tour: TourDef) =>
		!deps.isBusy() && offerableWhenIdle(tour);

	const arm = (routeName: string) => {
		pending = deps.setTimer(() => {
			pending = null;
			attempt(routeName);
		}, ROUTE_SETTLE_MS);
	};

	const attempt = (routeName: string) => {
		const onboarding = deps.onboarding();
		for (const tour of deps.tours()) {
			const trigger = tour.triggers.find(
				(t) => t.kind === 'first_visit' && t.routeName === routeName,
			);
			if (!trigger || trigger.kind !== 'first_visit') continue;
			if (trigger.unlessFact && onboarding?.facts[trigger.unlessFact]) continue;
			if (!offerableWhenIdle(tour)) continue;
			if (deps.isBusy()) return arm(routeName);
			write(deps.session, SESSION_KEY, true);
			tourTelemetry().track('tour_offered', {
				tour: tour.id,
				trigger: 'first_visit',
			});
			void deps
				.start(tour.id, 'first_visit')
				.then((started) => {
					if (started === false) write(deps.session, SESSION_KEY, false);
				})
				.catch(() => write(deps.session, SESSION_KEY, false));
			return;
		}
	};

	return {
		onRouteEntered(routeName) {
			pending?.();
			pending = null;
			if (routeName) arm(routeName);
		},

		noteSkip(id) {
			write(deps.storage, COOLDOWN_KEY, deps.now());
			const counts = readJson<Record<string, number>>(
				deps.storage,
				SKIPS_KEY,
				{},
			);
			counts[id] = (counts[id] ?? 0) + 1;
			write(deps.storage, SKIPS_KEY, counts);
		},

		canAutoOffer,
		skipCount,

		reset() {
			write(deps.storage, COOLDOWN_KEY, 0);
			write(deps.storage, SKIPS_KEY, {});
		},

		dispose() {
			pending?.();
			pending = null;
		},
	};
}
