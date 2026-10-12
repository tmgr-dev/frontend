import { effectScope, watch } from 'vue';
import type { Router } from 'vue-router';
import { activeLocalWorkspaceCode } from '@/local/runtime';
import { toursEngine } from '@/tours/useTours';
import type { OnboardingState } from '@/tours/types';
import { desktopWindowLabel, isDesktopApp } from '@/utils/desktop';
import { shouldShowConsentPanel } from '@/privacy/panelVisibility';
import { useConsent } from '@/privacy/useConsent';
import { clearFreshMarker, readFreshMarker, runLocalFirstRun } from './localFirstRun';
import { EXCLUDED_ROUTES, ONBOARDING_SINCE, shouldOfferIntent } from './redirect';

interface BootStore {
  state: any;
  getters: any;
  watch<T>(getter: () => T, cb: (value: T) => void, options?: { immediate?: boolean }): () => void;
}

interface WorkspaceLike {
  user_id?: number | string;
}

export interface OnboardingBootDeps {
  state(): OnboardingState | null;
  localOnly(): boolean;
  ready(): Promise<void>;
  routeName(): string | null;
  redirect(name: string): void;
  hasWorkspaceInvitation(): boolean;
  user(): { id?: number | string; created_at?: string | null };
  workspaces(): WorkspaceLike[];
  waitForSession(needWorkspaces: boolean): Promise<boolean>;
}

export function ownsOnlyDefaultWorkspace(
  workspaces: WorkspaceLike[],
  userId: number | string | undefined,
): boolean {
  if (userId === undefined || workspaces.length !== 1) return false;
  return String(workspaces[0].user_id) === String(userId);
}

export async function runOnboardingBoot(
  deps: OnboardingBootDeps,
): Promise<'redirected' | 'none'> {
  const state = deps.state();
  if (!state) return 'none';

  if (deps.localOnly()) return 'none';

  await deps.ready();
  if (!(await deps.waitForSession(state.invited === undefined))) return 'none';
  const user = deps.user();
  const offer = shouldOfferIntent({
    state: deps.state(),
    hasWorkspaceInvitation: deps.hasWorkspaceInvitation(),
    localOnly: false,
    mainWindow: true,
    routeName: deps.routeName(),
    userCreatedAt: user.created_at ?? null,
    onboardingSince: ONBOARDING_SINCE,
    ownsOnlyDefaultWorkspace: ownsOnlyDefaultWorkspace(deps.workspaces(), user.id),
  });
  if (!offer) return 'none';
  const route = deps.routeName();
  if (!route || EXCLUDED_ROUTES.includes(route)) return 'none';
  deps.redirect('WelcomeSetup');
  return 'redirected';
}

const SESSION_WAIT_MS = 5000;

export function installOnboardingFlow(store: BootStore, router: Router): () => void {
  if (isDesktopApp() && desktopWindowLabel() !== 'main') return () => {};

  const waitForSession = (needWorkspaces: boolean) =>
    new Promise<boolean>((resolve) => {
      const ready = () =>
        store.state.user?.id !== undefined &&
        (!needWorkspaces || (store.state.workspaces?.length ?? 0) > 0);
      if (ready()) return resolve(true);
      let stop = () => {};
      const timeout = setTimeout(() => {
        stop();
        resolve(false);
      }, SESSION_WAIT_MS);
      stop = store.watch(ready, (ok) => {
        if (!ok) return;
        clearTimeout(timeout);
        stop();
        resolve(true);
      });
    });

  let handled = false;
  const stop = watch(
    () => toursEngine().store.loaded.value,
    (loaded) => {
      if (!loaded) {
        handled = false;
        return;
      }
      if (handled) return;
      handled = true;
      const engine = toursEngine();
      void runOnboardingBoot({
        state: () => engine.store.state.value,
        localOnly: () => !!store.getters.isLocalOnly,
        ready: async () => void (await router.isReady()),
        routeName: () =>
          typeof router.currentRoute.value.name === 'string' ? router.currentRoute.value.name : null,
        redirect: (name) => void router.replace({ name }),
        hasWorkspaceInvitation: () => {
          try {
            return !!localStorage.getItem('workspace.invitation');
          } catch {
            return false;
          }
        },
        user: () => store.state.user ?? {},
        workspaces: () => store.state.workspaces ?? [],
        waitForSession,
      }).catch((error) => console.error('onboarding boot failed', error));
    },
    { immediate: true },
  );

  const waitConsentClosed = () =>
    new Promise<void>((resolve) => {
      const scope = effectScope();
      scope.run(() => {
        const { consent } = useConsent();
        const open = () =>
          !store.getters.canUseApp ||
          shouldShowConsentPanel({
            analytics: consent.value.analytics,
            usage: consent.value.usage,
            path: router.currentRoute.value.path,
            desktop: true,
            ready: store.getters.canUseApp,
          });
        watch(
          open,
          (isOpen) => {
            if (isOpen) return;
            scope.stop();
            resolve();
          },
          { immediate: true },
        );
      });
    });

  let firstRunHandled = false;
  const stopFirstRun = watch(
    () => toursEngine().store.loaded.value && !!store.getters.isLocalOnly,
    (ready) => {
      if (!ready) {
        firstRunHandled = false;
        return;
      }
      if (firstRunHandled) return;
      firstRunHandled = true;
      const engine = toursEngine();
      void runLocalFirstRun({
        localOnly: () => !!store.getters.isLocalOnly,
        mainWindow: () => !isDesktopApp() || desktopWindowLabel() === 'main',
        marker: () => readFreshMarker(localStorage),
        activeCode: () => activeLocalWorkspaceCode(),
        now: () => new Date(),
        clearMarker: () => clearFreshMarker(localStorage),
        state: () => engine.store.state.value,
        setIntent: (intent, options) => engine.api.setIntent(intent, options),
        waitConsentClosed,
        startSequence: (ids, trigger) => engine.api.startSequence(ids, trigger),
      }).catch((error) => console.error('local first run failed', error));
    },
    { immediate: true },
  );
  return () => {
    stop();
    stopFirstRun();
  };
}
