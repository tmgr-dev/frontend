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
  put(url: string, body?: unknown): Promise<{ data: unknown }>;
  delete(url: string): Promise<{ data: unknown }>;
}

export interface OnboardingApi {
  fetchState(): Promise<unknown>;
  putTour(id: string, body: TourWrite): Promise<unknown>;
  putPreferences(body: PreferencesWrite): Promise<unknown>;
  resetTours(): Promise<unknown>;
  fetchFacts(taskId: number | null, keys: string[]): Promise<Record<string, unknown>>;
}

const unwrap = (response: { data: unknown } | undefined): unknown => {
  const body = response?.data;
  if (body && typeof body === 'object' && !Array.isArray(body) && 'data' in body)
    return (body as { data: unknown }).data;
  return body;
};

export function createOnboardingApi(client: HttpClient): OnboardingApi {
  return {
    async fetchState() {
      return unwrap(await client.get('onboarding'));
    },
    async putTour(id, body) {
      return unwrap(await client.put(`onboarding/tours/${encodeURIComponent(id)}`, body));
    },
    async putPreferences(body) {
      return unwrap(await client.put('onboarding/preferences', body));
    },
    async resetTours() {
      return unwrap(await client.delete('onboarding/tours'));
    },
    async fetchFacts(taskId, keys) {
      const params: Record<string, string> = { keys: keys.join(',') };
      if (taskId != null) params.task_id = String(taskId);
      const data = unwrap(await client.get('onboarding/facts', { params }));
      return data && typeof data === 'object' && !Array.isArray(data)
        ? (data as Record<string, unknown>)
        : {};
    },
  };
}

let shared: OnboardingApi | null = null;

export function onboardingApi(): OnboardingApi {
  if (shared) return shared;
  const lazy = async () => (await import('@/plugins/axios')).default;
  shared = createOnboardingApi({
    get: async (url, config) => (await lazy()).get(url, config as never),
    put: async (url, body) => (await lazy()).put(url, body),
    delete: async (url) => (await lazy()).delete(url),
  });
  return shared;
}
