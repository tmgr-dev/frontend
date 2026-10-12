import type { Ref } from 'vue';
import type {
  OnboardingState,
  StepDef,
  TipsMode,
  TourDef,
  TourId,
  TourState,
} from './types';
import type { BodyPart } from './keycaps';

export interface ActiveTour {
  tour: TourDef;
  step: StepDef;
  index: number;
  total: number;
  waiting: boolean;
  unavailable?: boolean;
  alreadyDone?: boolean;
  canTakeMe?: boolean;
  onTargetPage?: boolean;
  mobile?: boolean;
  hint?: 'timer_too_short';
  bodyParts?: BodyPart[];
}

export interface HubItem {
  tour: TourDef;
  state: TourState | null;
  available: boolean;
  unavailableReason?: 'module_off' | 'module_on' | 'platform' | 'account' | 'role' | 'needs_persona';
}

export interface ToursApi {
  active: Readonly<Ref<ActiveTour | null>>;
  hubItems: Readonly<Ref<HubItem[]>>;
  onboarding: Readonly<Ref<OnboardingState | null>>;
  hostReady: Readonly<Ref<boolean>>;
  start(id: TourId, trigger: string): Promise<void>;
  resume(id: TourId): Promise<void>;
  pause(): void;
  next(): void;
  skipStep(): void;
  skipTour(): void;
  setTipsMode(mode: TipsMode): Promise<void>;
  setHubHidden(hidden: boolean): Promise<void>;
  startSequence(ids: TourId[], trigger: string): Promise<void>;
  offer(id: TourId, trigger: string, options?: { retryMs?: number }): Promise<boolean>;
  setIntent(intent: string, options?: { seed?: boolean }): Promise<void>;
  setChecklistHidden(hidden: boolean): Promise<void>;
  resetAll(): Promise<void>;
  takeMeThere(): void;
  ensurePersonas(): Promise<void>;
  patchFacts(facts: Record<string, unknown>): void;
  resumePill: Readonly<Ref<ResumePill | null>>;
  dismissPill(): void;
}

export interface ResumePill {
  tourId: TourId;
  title: string;
}

import type { OverlayAdapter } from './overlay/OverlayAdapter';
import { createTours } from './engine';
import type { OverlayRegistration, ToursEngine, ToursHostDeps } from './engine';
import { tourBus } from './events';
import { createOnboardingApi } from './api';

let overlay: OverlayRegistration | null = null;
let engine: ToursEngine | null = null;

export function setOverlayAdapter(
  adapter: OverlayAdapter | null,
  mount?: (host: HTMLElement) => () => void,
): void {
  overlay = adapter ? { adapter, mount } : null;
  engine?.rerender();
}

const inertDeps = (): ToursHostDeps => {
  const memory = () => {
    const data = new Map<string, string>();
    return {
      getItem: (k: string) => data.get(k) ?? null,
      setItem: (k: string, v: string) => void data.set(k, v),
    };
  };
  const none = async () => ({});
  return {
    api: createOnboardingApi({ get: none as never, post: none as never, put: none as never, delete: none as never }),
    bus: tourBus,
    lease: {
      acquire: () => true,
      renew: () => true,
      release() {},
      holds: () => true,
      broadcast() {},
      onMessage() {},
    },
    storage: memory(),
    session: memory(),
    now: Date.now,
    platform: () => 'web',
    mobile: () => false,
    account: () => 'cloud',
    isOwner: () => true,
    modulesKnown: () => false,
    isModuleEnabled: () => true,
    currentRouteName: () => null,
    navigate() {},
    isBusy: () => false,
    overlay: () => overlay,
    setTimer: (fn, ms) => {
      const id = setTimeout(fn, ms);
      return () => clearTimeout(id);
    },
  };
};

export function configureTours(deps: Omit<ToursHostDeps, 'overlay'>): ToursEngine {
  engine?.dispose();
  engine = createTours({ ...deps, overlay: () => overlay });
  return engine;
}

export function toursEngine(): ToursEngine {
  return engine ?? configureTours(inertDeps());
}

export function resetToursForTests(): void {
  engine?.dispose();
  engine = null;
  overlay = null;
}

export function useTours(): ToursApi {
  return toursEngine().api;
}
