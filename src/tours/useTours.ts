import type { Ref } from 'vue';
import type {
  OnboardingState,
  StepDef,
  TipsMode,
  TourDef,
  TourId,
  TourState,
} from './types';

export interface ActiveTour {
  tour: TourDef;
  step: StepDef;
  index: number;
  total: number;
  waiting: boolean;
  unavailable?: boolean;
  alreadyDone?: boolean;
  canTakeMe?: boolean;
  mobile?: boolean;
  hint?: 'timer_too_short';
}

export interface HubItem {
  tour: TourDef;
  state: TourState | null;
  available: boolean;
  unavailableReason?: 'module_off' | 'platform' | 'account' | 'role';
}

export interface ToursApi {
  active: Readonly<Ref<ActiveTour | null>>;
  hubItems: Readonly<Ref<HubItem[]>>;
  onboarding: Readonly<Ref<OnboardingState | null>>;
  start(id: TourId, trigger: string): Promise<void>;
  resume(id: TourId): Promise<void>;
  pause(): void;
  next(): void;
  skipStep(): void;
  skipTour(): void;
  setTipsMode(mode: TipsMode): Promise<void>;
  resetAll(): Promise<void>;
}

export function useTours(): ToursApi {
  throw new Error('not implemented');
}
