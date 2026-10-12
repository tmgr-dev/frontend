import type { OnboardingState, TipsMode, TourState, TourStatus } from '@/tours/types';

export const ONBOARDING_KEY = 'tmgr:onboarding:v1';

export interface OnboardingStorage {
	getItem(key: string): string | null;
	setItem(key: string, value: string): void;
}

export interface OnboardingResult {
	status: number;
	data: any;
}

const TIPS_MODES: TipsMode[] = ['auto', 'hub_only', 'off'];
const STATUSES: TourStatus[] = ['in_progress', 'paused', 'completed', 'skipped'];
const INTENTS = ['human', 'agent', 'skipped'];

const initial = (): OnboardingState => ({
	intent: null,
	tips_mode: 'auto',
	hub_hidden: false,
	checklist_hidden: false,
	tours_generation: 0,
	tours: {},
	facts: {},
});

const load = (storage: OnboardingStorage): OnboardingState => {
	try {
		const raw = storage.getItem(ONBOARDING_KEY);
		const value = raw ? JSON.parse(raw) : null;
		if (value && typeof value === 'object' && !Array.isArray(value)) {
			return { ...initial(), ...value, facts: {} };
		}
	} catch {
		/* corrupt value: start over */
	}
	return initial();
};

const ok = (state: OnboardingState): OnboardingResult => ({ status: 200, data: { data: state } });
const preferences = (state: OnboardingState): OnboardingResult => ({
	status: 200,
	data: {
		data: {
			tips_mode: state.tips_mode,
			hub_hidden: state.hub_hidden,
			checklist_hidden: state.checklist_hidden,
		},
	},
});
const invalid = (message: string): OnboardingResult => ({ status: 422, data: { message } });

export const createOnboardingHandler = (storage: OnboardingStorage) => {
	const save = (state: OnboardingState) => {
		try {
			storage.setItem(ONBOARDING_KEY, JSON.stringify({ ...state, facts: undefined }));
		} catch {
			/* storage unavailable: the choice lasts until restart */
		}
	};

	return (method: string, path: string, body?: any): OnboardingResult => {
		const verb = method.toUpperCase();
		const payload = body && typeof body === 'object' ? body : {};
		const state = load(storage);

		if (path === 'onboarding' && verb === 'GET') {
			if (state.intent === null) {
				state.intent = 'human';
				save(state);
			}
			return ok({ ...state, invited: false });
		}

		if (path === 'onboarding/facts' && verb === 'GET') return { status: 200, data: { data: {} } };

		if (path === 'onboarding/preferences' && verb === 'PUT') {
			if (TIPS_MODES.includes(payload.tips_mode)) state.tips_mode = payload.tips_mode;
			if (typeof payload.hub_hidden === 'boolean') state.hub_hidden = payload.hub_hidden;
			if (typeof payload.checklist_hidden === 'boolean') state.checklist_hidden = payload.checklist_hidden;
			save(state);
			return preferences(state);
		}

		if (path === 'onboarding/intent' && verb === 'POST') {
			if (!INTENTS.includes(payload.intent)) return invalid('Unknown intent');
			state.intent = payload.intent;
			save(state);
			return { status: 200, data: { data: { intent: state.intent, starter_tasks_created: 0, invited: false } } };
		}

		if (path === 'onboarding/starter-tasks' && verb === 'DELETE') {
			return { status: 200, data: { data: { removed: 0 } } };
		}

		if (path === 'onboarding/tours' && verb === 'DELETE') {
			state.tours = {};
			state.tours_generation += 1;
			save(state);
			return { status: 200, data: { data: { tours_generation: state.tours_generation } } };
		}

		const tour = /^onboarding\/tours\/([^/]+)$/.exec(path);
		if (tour && verb === 'PUT') {
			const id = decodeURIComponent(tour[1]);
			if (!STATUSES.includes(payload.status)) return invalid('Unknown tour status');
			const generation = Number(payload.generation ?? 0);
			const version = Number(payload.version ?? 1);
			if (generation < state.tours_generation) return { status: 200, data: { data: state.tours[id] ?? null } };
			const existing: TourState | undefined = state.tours[id];
			const downgrade =
				existing?.status === 'completed' &&
				payload.status !== 'completed' &&
				(generation <= existing.generation || version < existing.version);
			if (!downgrade) {
				state.tours[id] = { status: payload.status, step_id: payload.step_id ?? null, version, generation };
				save(state);
			}
			return { status: 200, data: { data: state.tours[id] ?? null } };
		}

		return { status: 404, data: { message: 'Not found' } };
	};
};
