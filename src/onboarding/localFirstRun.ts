import type { OnboardingState } from '@/tours/types';

export const LOCAL_FRESH_KEY = 'tmgr:onboarding:local-fresh';
const ONBOARDING_STATE_KEY = 'tmgr:onboarding:v1';

export const LOCAL_FIRST_RUN_SEQUENCE = [
  'desktop.local-workspace',
  'start.first-task',
  'time.timer',
  'desktop.quick-add',
];

export interface FreshMarker {
  code: string;
  at: string;
}

export interface MarkerStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export interface FreshInstallFacts {
  hasProfile: boolean;
  workspaceCount: number;
  onboardingRaw: string | null;
}

const hasStarterSeededAt = (raw: string | null): boolean => {
  if (!raw) return false;
  try {
    const value = JSON.parse(raw);
    return !!value && typeof value === 'object' && !!value.starter_seeded_at;
  } catch {
    return false;
  }
};

export function isFreshLocalInstall(facts: FreshInstallFacts): boolean {
  return (
    !facts.hasProfile && facts.workspaceCount === 0 && !hasStarterSeededAt(facts.onboardingRaw)
  );
}

export function writeFreshMarker(storage: MarkerStorage, code: string, at: Date): void {
  try {
    storage.setItem(LOCAL_FRESH_KEY, JSON.stringify({ code, at: at.toISOString() }));
  } catch {
    /* storage unavailable: no first-run tours */
  }
}

export function readFreshMarker(storage: MarkerStorage): FreshMarker | null {
  try {
    const raw = storage.getItem(LOCAL_FRESH_KEY);
    const value = raw ? JSON.parse(raw) : null;
    if (value && typeof value.code === 'string' && typeof value.at === 'string') return value;
  } catch {
    /* unreadable marker counts as none */
  }
  return null;
}

export function clearFreshMarker(storage: MarkerStorage): void {
  try {
    storage.removeItem(LOCAL_FRESH_KEY);
  } catch {
    /* nothing to clear */
  }
}

export function freshInstallFacts(
  storage: MarkerStorage,
  hasProfile: boolean,
  workspaceCount: number,
): FreshInstallFacts {
  let onboardingRaw: string | null = null;
  try {
    onboardingRaw = storage.getItem(ONBOARDING_STATE_KEY);
  } catch {
    /* treated as no state */
  }
  return { hasProfile, workspaceCount, onboardingRaw };
}

export interface LocalFirstRunContext {
  localOnly: boolean;
  mainWindow: boolean;
  marker: FreshMarker | null;
  state: OnboardingState | null | undefined;
}

export function shouldRunLocalFirstRun(ctx: LocalFirstRunContext): boolean {
  const state = ctx.state;
  if (!ctx.localOnly || !ctx.mainWindow || !ctx.marker) return false;
  if (!state || typeof state !== 'object' || Array.isArray(state) || state.malformed === true) {
    return false;
  }
  return state.intent === null || state.intent === 'human';
}

export interface LocalFirstRunDeps {
  localOnly(): boolean;
  mainWindow(): boolean;
  marker(): FreshMarker | null;
  clearMarker(): void;
  state(): OnboardingState | null | undefined;
  setIntent(intent: string, options: { seed: boolean }): Promise<void>;
  waitConsentClosed(): Promise<void>;
  startSequence(ids: string[], trigger: string): Promise<void>;
}

export async function runLocalFirstRun(deps: LocalFirstRunDeps): Promise<'started' | 'seeded' | 'none'> {
  if (
    !shouldRunLocalFirstRun({
      localOnly: deps.localOnly(),
      mainWindow: deps.mainWindow(),
      marker: deps.marker(),
      state: deps.state(),
    })
  ) {
    return 'none';
  }
  if (deps.state()?.intent === null) await deps.setIntent('human', { seed: true });
  if (deps.state()?.tips_mode !== 'auto') {
    deps.clearMarker();
    return 'seeded';
  }
  await deps.waitConsentClosed();
  await deps.startSequence(LOCAL_FIRST_RUN_SEQUENCE, 'first_run');
  deps.clearMarker();
  return 'started';
}
