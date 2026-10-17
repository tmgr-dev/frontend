import type { TipsMode, TourStatus } from './types';

export interface TourWrite {
	status: TourStatus;
	step_id: string | null;
	version: number;
	platform: string;
	generation: number;
}

export interface PreferencesWrite {
	tips_mode?: TipsMode;
	hub_hidden?: boolean;
	checklist_hidden?: boolean;
}

interface HttpClient {
	get(url: string, config?: unknown): Promise<{ data: unknown }>;
	post(url: string, body?: unknown): Promise<{ data: unknown }>;
	put(url: string, body?: unknown): Promise<{ data: unknown }>;
	delete(url: string): Promise<{ data: unknown }>;
}

export type OnboardingFacts = Record<string, unknown>;

export interface OnboardingApi {
	fetchState(): Promise<unknown>;
	putTour(id: string, body: TourWrite): Promise<unknown>;
	putPreferences(body: PreferencesWrite): Promise<unknown>;
	resetTours(): Promise<unknown>;
	setIntent(
		intent: string,
		options?: { seed?: boolean; workspace?: string },
	): Promise<unknown>;
	fetchFacts(
		keys: string[],
		taskId?: number | string,
	): Promise<OnboardingFacts>;
	removeStarterTasks(): Promise<{ removed: number }>;
}

const unwrap = (response: { data: unknown } | undefined): unknown => {
	const body = response?.data;
	if (
		body &&
		typeof body === 'object' &&
		!Array.isArray(body) &&
		'data' in body
	)
		return (body as { data: unknown }).data;
	return body;
};

export function createOnboardingApi(client: HttpClient): OnboardingApi {
	return {
		async fetchState() {
			return unwrap(await client.get('onboarding'));
		},
		async putTour(id, body) {
			return unwrap(
				await client.put(`onboarding/tours/${encodeURIComponent(id)}`, body),
			);
		},
		async putPreferences(body) {
			return unwrap(await client.put('onboarding/preferences', body));
		},
		async resetTours() {
			return unwrap(await client.delete('onboarding/tours'));
		},
		async setIntent(intent, options) {
			const body: Record<string, unknown> = { intent };
			if (options?.seed !== undefined) body.seed = options.seed;
			if (options?.workspace !== undefined) body.workspace = options.workspace;
			return unwrap(await client.post('onboarding/intent', body));
		},
		async fetchFacts(keys, taskId) {
			const params: Record<string, unknown> = { keys: keys.join(',') };
			if (taskId !== undefined) params.task_id = taskId;
			const body = unwrap(await client.get('onboarding/facts', { params }));
			return body && typeof body === 'object' && !Array.isArray(body)
				? (body as OnboardingFacts)
				: {};
		},
		async removeStarterTasks() {
			const body = unwrap(await client.delete('onboarding/starter-tasks'));
			const removed = Number((body as { removed?: unknown } | null)?.removed);
			return { removed: Number.isFinite(removed) ? removed : 0 };
		},
	};
}

let shared: OnboardingApi | null = null;

export function onboardingApi(): OnboardingApi {
	if (shared) return shared;
	const lazy = async () => (await import('@/plugins/axios')).default;
	shared = createOnboardingApi({
		get: async (url, config) => (await lazy()).get(url, config as never),
		post: async (url, body) => (await lazy()).post(url, body),
		put: async (url, body) => (await lazy()).put(url, body),
		delete: async (url) => (await lazy()).delete(url),
	});
	return shared;
}
