export type TourId = string;

export type Platform =
  | 'web'
  | 'mobile_web'
  | 'desktop'
  | 'desktop_local'
  | 'ios';

export type TourGroup =
  | 'start'
  | 'tasks'
  | 'time'
  | 'agents'
  | 'personas'
  | 'pages'
  | 'graph'
  | 'routines'
  | 'workspace'
  | 'desktop';

export type TourTrigger =
  | { kind: 'first_run'; route?: 'human' | 'agent' }
  | { kind: 'first_visit'; routeName: string; unlessFact?: string }
  | { kind: 'feature_enabled'; module: string }
  | {
      kind: 'event';
      event: string;
      count?: number;
      where?: Record<string, string | number>;
    }
  | { kind: 'hub' }
  | { kind: 'whats_new' };

export interface TourRequires {
  modules?: string[];
  platforms?: Platform[];
  account?: 'cloud' | 'local' | 'any';
  role?: 'owner' | 'member';
  capability?: 'localAccess' | 'pluginWindows';
}

export interface StepDef {
  id: string;
  type: 'tip' | 'spot' | 'modal' | 'wait';
  anchor?: string;
  mobileAnchor?: string;
  anchorScope?: 'eventTask';
  requiresModules?: string[];
  route?: { name: string; params?: 'currentWorkspace' };
  takeMeRoute?: { name: string; params?: 'currentWorkspace' };
  title: string;
  body: string;
  waitFor?: { event: string; where?: Record<string, string | number> };
  alreadyDone?: string;
  optional?: boolean;
  mobileBody?: string;
}

export interface TourDef {
  id: TourId;
  version: number;
  since: string;
  group: TourGroup;
  title: string;
  minutes: number;
  triggers: TourTrigger[];
  requires?: TourRequires;
  steps: StepDef[];
  completeOn?: string;
}

export type TourStatus = 'in_progress' | 'paused' | 'completed' | 'skipped';

export interface TourState {
  status: TourStatus;
  step_id: string | null;
  version: number;
  generation: number;
}

export type TipsMode = 'auto' | 'hub_only' | 'off';

export interface OnboardingState {
  intent: string | null;
  tips_mode: TipsMode;
  hub_hidden: boolean;
  checklist_hidden: boolean;
  tours_generation: number;
  tours: Record<TourId, TourState>;
  facts: Record<string, unknown>;
}
