import { ref } from 'vue';
import type { Ref } from 'vue';
import type { OnboardingApi, PreferencesWrite } from './api';
import type { OnboardingState, TipsMode, TourId, TourState, TourStatus } from './types';

const STATUSES: TourStatus[] = ['in_progress', 'paused', 'completed', 'skipped'];
const TIPS_MODES: TipsMode[] = ['auto', 'hub_only', 'off'];

const isRecord = (value: unknown): value is Record<string, any> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

export const emptyOnboarding = (): OnboardingState => ({
  intent: null,
  tips_mode: 'auto',
  hub_hidden: false,
  checklist_hidden: false,
  tours_generation: 0,
  tours: {},
  facts: {},
});

export function normalizeOnboarding(raw: unknown): OnboardingState {
  const state = emptyOnboarding();
  if (!isRecord(raw)) return { ...state, malformed: true };
  state.intent = typeof raw.intent === 'string' ? raw.intent : null;
  if (typeof raw.invited === 'boolean') state.invited = raw.invited;
  if (TIPS_MODES.includes(raw.tips_mode)) state.tips_mode = raw.tips_mode;
  state.hub_hidden = raw.hub_hidden === true;
  state.checklist_hidden = raw.checklist_hidden === true;
  const generation = Number(raw.tours_generation);
  state.tours_generation = Number.isFinite(generation) && generation > 0 ? generation : 0;
  if (isRecord(raw.facts)) state.facts = raw.facts;
  if (isRecord(raw.tours)) {
    for (const [id, entry] of Object.entries(raw.tours)) {
      if (!isRecord(entry) || !STATUSES.includes(entry.status)) continue;
      const entryGeneration = Number(entry.generation);
      const tourGeneration = Number.isFinite(entryGeneration)
        ? entryGeneration
        : state.tours_generation;
      if (tourGeneration < state.tours_generation) continue;
      const version = Number(entry.version);
      state.tours[id] = {
        status: entry.status,
        step_id: typeof entry.step_id === 'string' ? entry.step_id : null,
        version: Number.isFinite(version) && version > 0 ? version : 1,
        generation: tourGeneration,
      };
    }
  }
  return state;
}

const backendPlatform = (platform: string): string => {
  if (platform === 'desktop_local' || platform === 'desktop') return 'desktop';
  if (platform === 'ios') return 'ios';
  return 'web';
};

export interface SaveTourInput {
  status: TourStatus;
  step_id: string | null;
  version: number;
}

export interface ToursStoreDeps {
  api: OnboardingApi;
  platform: () => string;
  onSaved?: (id: TourId, state: TourState) => void;
}

export interface ToursStore {
  state: Ref<OnboardingState | null>;
  loaded: Ref<boolean>;
  load(): Promise<OnboardingState | null>;
  clear(): void;
  tourState(id: TourId): TourState | null;
  saveTour(id: TourId, input: SaveTourInput): TourState | null;
  applyRemote(id: TourId, remote: TourState): boolean;
  setPreferences(patch: PreferencesWrite): Promise<void>;
  resetAll(): Promise<void>;
  applyIntent(intent: string, options?: { invited?: boolean }): void;
  setChecklistHidden(hidden: boolean): Promise<void>;
  patchFacts(facts: Record<string, unknown>): void;
  flush(): Promise<void>;
}

export function createToursStore(deps: ToursStoreDeps): ToursStore {
  const state = ref<OnboardingState | null>(null);
  const loaded = ref(false);
  const queues = new Map<TourId, Promise<unknown>>();
  let epoch = 0;

  const ensure = (): OnboardingState => {
    if (!state.value) state.value = emptyOnboarding();
    return state.value;
  };

  const wouldDowngrade = (current: TourState | undefined, status: TourStatus, version: number) =>
    current?.status === 'completed' && status !== 'completed' && version <= current.version;

  const enqueue = (id: TourId, job: () => Promise<unknown>) => {
    const next = (queues.get(id) ?? Promise.resolve()).then(job).catch((error) => {
      console.error('tour state write failed', error);
    });
    queues.set(id, next);
  };

  return {
    state,
    loaded,

    async load() {
      const startedAt = epoch;
      let raw: unknown;
      try {
        raw = await deps.api.fetchState();
      } catch (error) {
        console.error('onboarding load failed', error);
        return null;
      }
      if (startedAt !== epoch) return null;
      const incoming = normalizeOnboarding(raw);
      const current = state.value;
      if (current && incoming.tours_generation < current.tours_generation) {
        incoming.tours_generation = current.tours_generation;
        incoming.tours = {};
      }
      state.value = incoming;
      loaded.value = true;
      return incoming;
    },

    clear() {
      epoch += 1;
      state.value = null;
      loaded.value = false;
      queues.clear();
    },

    tourState(id) {
      return state.value?.tours[id] ?? null;
    },

    saveTour(id, input) {
      const current = ensure();
      if (wouldDowngrade(current.tours[id], input.status, input.version)) return null;
      const next: TourState = {
        status: input.status,
        step_id: input.step_id,
        version: input.version,
        generation: current.tours_generation,
      };
      current.tours[id] = next;
      enqueue(id, () =>
        deps.api.putTour(id, { ...next, platform: backendPlatform(deps.platform()) }),
      );
      deps.onSaved?.(id, next);
      return next;
    },

    applyRemote(id, remote) {
      const current = ensure();
      if (remote.generation < current.tours_generation) return false;
      if (remote.generation > current.tours_generation) {
        current.tours_generation = remote.generation;
        current.tours = {};
      }
      if (wouldDowngrade(current.tours[id], remote.status, remote.version)) return false;
      current.tours[id] = { ...remote };
      return true;
    },

    async setPreferences(patch) {
      const current = ensure();
      const before = {
        tips_mode: current.tips_mode,
        hub_hidden: current.hub_hidden,
        checklist_hidden: current.checklist_hidden,
      };
      Object.assign(current, patch);
      try {
        await deps.api.putPreferences(patch);
      } catch (error) {
        Object.assign(current, before);
        throw error;
      }
    },

    async resetAll() {
      const current = ensure();
      const result = await deps.api.resetTours();
      const generation = isRecord(result) ? Number(result.tours_generation) : NaN;
      current.tours = {};
      current.tours_generation = Number.isFinite(generation)
        ? Math.max(generation, current.tours_generation)
        : current.tours_generation + 1;
    },

    applyIntent(intent, options) {
      const current = ensure();
      current.intent = intent;
      if (options?.invited !== undefined) current.invited = options.invited;
    },

    async setChecklistHidden(hidden) {
      return this.setPreferences({ checklist_hidden: hidden });
    },

    patchFacts(facts) {
      const current = ensure();
      current.facts = { ...current.facts, ...facts };
    },

    async flush() {
      await Promise.all([...queues.values()]);
    },
  };
}
