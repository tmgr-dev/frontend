import { computed, ref } from 'vue';
import type { Ref } from 'vue';
import type { OverlayAdapter } from './overlay/OverlayAdapter';
import { resolveAnchor as defaultResolveAnchor } from './anchors';
import { getTour, TOURS } from './catalog';
import type { OnboardingApi } from './api';
import type { TourBus } from './events';
import { TourRunner } from './runner';
import type { RunnerView } from './runner';
import { createToursStore } from './store';
import type { ToursStore } from './store';
import { tourTelemetry } from './telemetry';
import { createTriggers, SESSION_KEY } from './triggers';
import type { Triggers } from './triggers';
import type { TourLease } from './lease';
import type { Platform, StepDef, TipsMode, TourDef, TourId, TourState } from './types';
import type { ActiveTour, HubItem, ResumePill, ToursApi } from './useTours';

export interface OverlayRegistration {
  adapter: OverlayAdapter;
  mount?: (host: HTMLElement) => () => void;
}

export interface ToursHostDeps {
  api: OnboardingApi;
  bus: TourBus;
  lease: TourLease;
  storage: { getItem(k: string): string | null; setItem(k: string, v: string): void };
  session: { getItem(k: string): string | null; setItem(k: string, v: string): void };
  now(): number;
  platform(): Platform;
  mobile(): boolean;
  account(): 'cloud' | 'local';
  isOwner(): boolean;
  modulesKnown(): boolean;
  isModuleEnabled(key: string): boolean;
  currentRouteName(): string | null;
  navigate(route: NonNullable<StepDef['route']>): void;
  isBusy(): boolean;
  overlay(): OverlayRegistration | null;
  setTimer(fn: () => void, ms: number): () => void;
  facts?(): Record<string, unknown>;
  resolveAnchor?: typeof defaultResolveAnchor;
}

export interface ToursEngine {
  api: ToursApi;
  runner: TourRunner;
  store: ToursStore;
  triggers: Triggers;
  init(): Promise<void>;
  signedOut(): void;
  routeEntered(name: string | null): void;
  modulesChanged(): void;
  rerender(): void;
  dispose(): void;
}

const LEASE_RENEW_MS = 10_000;
const WATCH_ANCHOR_MS = 10 * 60_000;

export function createTours(deps: ToursHostDeps): ToursEngine {
  const active = ref<ActiveTour | null>(null) as Ref<ActiveTour | null>;
  const resumePill = ref<ResumePill | null>(null) as Ref<ResumePill | null>;
  const hostReady = ref(!!deps.overlay());
  let lastView: RunnerView | null = null;
  let shownKey = '';
  let cancelRenew: (() => void) | null = null;
  let sequence: { current: TourId; rest: TourId[]; trigger: string } | null = null;

  const store = createToursStore({
    api: deps.api,
    platform: () => deps.platform(),
    onSaved: (id, state) => deps.lease.broadcast({ type: 'tour_state', id, state }),
  });

  deps.lease.onMessage((message) => {
    if (message.type !== 'tour_state' || typeof message.id !== 'string') return;
    store.applyRemote(message.id, message.state as TourState);
  });

  const unavailableReason = (tour: TourDef): HubItem['unavailableReason'] | undefined => {
    const req = tour.requires;
    if (req?.platforms && !req.platforms.includes(deps.platform())) return 'platform';
    if (req?.account && req.account !== 'any' && req.account !== deps.account()) return 'account';
    if (req?.role === 'owner' && !deps.isOwner()) return 'role';
    if (deps.modulesKnown() && (req?.modules ?? []).some((key) => !deps.isModuleEnabled(key)))
      return 'module_off';
    return undefined;
  };

  const hubItems = computed<HubItem[]>(() =>
    TOURS.map((tour) => {
      const reason = unavailableReason(tour);
      return {
        tour,
        state: store.state.value?.tours[tour.id] ?? null,
        available: !reason,
        ...(reason ? { unavailableReason: reason } : {}),
      };
    }),
  );

  const stopRenewing = () => {
    cancelRenew?.();
    cancelRenew = null;
  };

  const keepLease = () => {
    stopRenewing();
    const tick = () => {
      cancelRenew = deps.setTimer(() => {
        if (runner.activeTourId) {
          if (typeof document === 'undefined' || document.visibilityState !== 'hidden')
            deps.lease.renew();
          tick();
        }
      }, LEASE_RENEW_MS);
    };
    tick();
  };

  const showView = (view: RunnerView | null) => {
    const registration = deps.overlay();
    if (!registration) return;
    if (!view) {
      shownKey = '';
      registration.adapter.hide();
      return;
    }
    const key = `${view.tour.id}/${view.step.id}/${view.mode}/${view.unavailable}/${view.alreadyDone}`;
    const same = key === shownKey && lastShownAnchor === view.anchorEl;
    shownKey = key;
    lastShownAnchor = view.anchorEl;
    if (same) registration.adapter.refresh();
    else
      registration.adapter.show({
        anchorEl: view.anchorEl,
        mode: view.mode,
        blocking: !view.unavailable,
        mount: registration.mount ?? (() => () => {}),
      });
  };
  let lastShownAnchor: Element | null = null;

  const runner: TourRunner = new TourRunner({
    getTour,
    stateOf: (id) => store.tourState(id),
    now: deps.now,
    mobile: deps.mobile,
    modulesKnown: deps.modulesKnown,
    isModuleEnabled: deps.isModuleEnabled,
    currentRouteName: deps.currentRouteName,
    navigate: deps.navigate,
    resolveAnchor: (step, options) => (deps.resolveAnchor ?? defaultResolveAnchor)(step, options),
    waitForAnchor: (step, options) =>
      (deps.resolveAnchor ?? defaultResolveAnchor)(step, { ...options, timeoutMs: WATCH_ANCHOR_MS }),
    facts: () => ({ ...(store.state.value?.facts ?? {}), ...(deps.facts?.() ?? {}) }),
    save: (id, input) => void store.saveTour(id, input),
    bus: deps.bus,
    setTimer: deps.setTimer,
    eligible: (tour) => !unavailableReason(tour),
    render: (view) => {
      lastView = view;
      if (!view) {
        active.value = null;
      } else {
        const { anchorEl: _anchorEl, mode: _mode, ...rest } = view;
        active.value = rest;
      }
      showView(view);
    },
    onFinished: (id, reason) => {
      stopRenewing();
      deps.lease.release();
      const tour = getTour(id);
      resumePill.value = reason === 'navigated' && tour ? { tourId: id, title: tour.title } : null;
      const chain = sequence;
      if (!chain || chain.current !== id) return;
      if (reason === 'completed') void runSequence(chain.rest, chain.trigger);
      else sequence = null;
    },
    onSkip: (id) => triggers.noteSkip(id),
  });

  const triggers = createTriggers({
    tours: () => TOURS,
    onboarding: () => store.state.value,
    tourState: (id) => store.tourState(id),
    now: deps.now,
    storage: deps.storage,
    session: deps.session,
    isBusy: deps.isBusy,
    hasActive: () => runner.activeTourId !== null,
    eligible: (tour) => !unavailableReason(tour),
    start: (id, trigger) => begin(id, trigger, false),
    setTimer: deps.setTimer,
  });

  async function begin(id: TourId, trigger: string, resume: boolean): Promise<boolean> {
    if (!deps.lease.acquire()) return false;
    resumePill.value = null;
    const started = resume ? await runner.resume(id) : await runner.start(id, trigger);
    if (started) keepLease();
    else deps.lease.release();
    return started;
  }

  const markAutoUsed = (used: boolean) => {
    try {
      deps.session.setItem(SESSION_KEY, JSON.stringify(used));
    } catch {
      return;
    }
  };

  async function runSequence(ids: TourId[], trigger: string): Promise<boolean> {
    for (let i = 0; i < ids.length; i++) {
      sequence = { current: ids[i], rest: ids.slice(i + 1), trigger };
      if (await begin(ids[i], trigger, false)) return true;
    }
    sequence = null;
    return false;
  }

  const api: ToursApi = {
    active,
    hubItems: hubItems as unknown as ToursApi['hubItems'],
    onboarding: store.state,
    hostReady,
    resumePill,
    start: async (id, trigger) => void (await begin(id, trigger, false)),
    resume: async (id) => void (await begin(id, 'resume', true)),
    pause: () => runner.pause(),
    next: () => void runner.next(),
    skipStep: () => void runner.skipStep(),
    skipTour: () => runner.skipTour(),
    takeMeThere: () => void runner.takeMeThere(),
    dismissPill: () => {
      resumePill.value = null;
    },
    async setTipsMode(mode: TipsMode) {
      await store.setPreferences({ tips_mode: mode });
      tourTelemetry().track('tips_mode_changed', { mode });
    },
    async setHubHidden(hidden: boolean) {
      await store.setPreferences({ hub_hidden: hidden });
    },
    async startSequence(ids, trigger) {
      if (store.state.value?.tips_mode !== 'auto') return;
      markAutoUsed(true);
      if (!(await runSequence(ids, trigger))) markAutoUsed(false);
    },
    async setIntent(intent, options) {
      const before = store.state.value;
      const previous = before ? { intent: before.intent, invited: before.invited } : null;
      store.applyIntent(intent);
      let result: unknown;
      try {
        result = await deps.api.setIntent(intent, options);
      } catch (error) {
        const current = store.state.value;
        if (current && previous) Object.assign(current, previous);
        else if (current) current.intent = null;
        throw error;
      }
      const reply = (typeof result === 'object' && result !== null ? result : {}) as Record<string, unknown>;
      if (typeof reply.invited === 'boolean') store.applyIntent(intent, { invited: reply.invited });
      if (options?.seed === false && Number(reply.starter_tasks_created) > 0) {
        try {
          await deps.api.removeStarterTasks();
        } catch (error) {
          console.error('removing starter tasks failed', error);
        }
      }
      if (intent === 'skipped' || intent === 'agent') markAutoUsed(true);
    },
    async setChecklistHidden(hidden: boolean) {
      await store.setChecklistHidden(hidden);
    },
    async resetAll() {
      sequence = null;
      runner.abandon();
      await store.resetAll();
      triggers.reset();
      resumePill.value = null;
    },
  };

  return {
    api,
    runner,
    store,
    triggers,
    async init() {
      await store.load();
      const route = deps.currentRouteName();
      if (route) triggers.onRouteEntered(route);
    },
    signedOut() {
      sequence = null;
      runner.abandon();
      triggers.dispose();
      store.clear();
      resumePill.value = null;
    },
    routeEntered(name) {
      runner.routeChanged(name);
      triggers.onRouteEntered(name);
    },
    modulesChanged() {
      runner.recheck();
    },
    rerender() {
      hostReady.value = !!deps.overlay();
      showView(lastView);
    },
    dispose() {
      sequence = null;
      runner.dispose();
      triggers.dispose();
      stopRenewing();
    },
  };
}
