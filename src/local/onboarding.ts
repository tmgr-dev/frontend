import type { OnboardingState, TipsMode, TourState, TourStatus } from '@/tours/types';
import { FACT_KEYS, computeFacts, removeStarters, seedStarters, type TokenLike } from './onboardingFacts';
import type { LocalContext } from './types';

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

export interface OnboardingDeps {
	ctxFor(code?: string): Promise<LocalContext | null>;
	tokens(): Promise<TokenLike[]>;
}

type LocalOnboardingState = OnboardingState & { starter_seeded_at?: string; starter_workspace?: string };

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

export const createOnboardingHandler = (storage: OnboardingStorage, deps?: OnboardingDeps) => {
	const save = (state: OnboardingState) => {
		try {
			storage.setItem(ONBOARDING_KEY, JSON.stringify({ ...state, facts: undefined }));
		} catch {
			/* storage unavailable: the choice lasts until restart */
		}
	};

	const factsFor = async (keys: readonly string[], taskId?: number): Promise<Record<string, boolean>> => {
		if (!deps) return {};
		try {
			const ctx = await deps.ctxFor();
			return ctx ? await computeFacts(ctx, keys, deps.tokens, taskId) : {};
		} catch (error) {
			console.error('local onboarding facts failed', error);
			return {};
		}
	};

	const seed = async (state: LocalOnboardingState): Promise<number> => {
		if (!deps || state.starter_seeded_at) return 0;
		try {
			const ctx = await deps.ctxFor();
			if (!ctx) return 0;
			const created = await seedStarters(ctx);
			state.starter_seeded_at = ctx.now().toISOString();
			state.starter_workspace = ctx.workspace.code;
			return created;
		} catch (error) {
			console.error('local starter tasks failed', error);
			return 0;
		}
	};

	return async (
		method: string,
		path: string,
		body?: any,
		params?: Record<string, unknown>,
	): Promise<OnboardingResult> => {
		const verb = method.toUpperCase();
		const payload = body && typeof body === 'object' ? body : {};
		const state: LocalOnboardingState = load(storage);

		if (path === 'onboarding' && verb === 'GET') return ok({ ...state, facts: await factsFor(FACT_KEYS) });

		if (path === 'onboarding/facts' && verb === 'GET') {
			const keys = String(params?.keys ?? '')
				.split(',')
				.map((key) => key.trim())
				.filter(Boolean);
			const taskId = params?.task_id === undefined ? undefined : Number(params.task_id);
			return { status: 200, data: { data: await factsFor(keys.length ? keys : FACT_KEYS, taskId) } };
		}

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
			const created = payload.intent === 'human' && payload.seed !== false ? await seed(state) : 0;
			save(state);
			return { status: 200, data: { data: { intent: state.intent, starter_tasks_created: created, invited: false } } };
		}

		if (path === 'onboarding/starter-tasks' && verb === 'DELETE') {
			let removed = 0;
			if (deps && state.starter_workspace) {
				try {
					const ctx = await deps.ctxFor(state.starter_workspace);
					if (ctx) removed = await removeStarters(ctx);
				} catch (error) {
					console.error('removing local starter tasks failed', error);
				}
			}
			return { status: 200, data: { data: { removed } } };
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
