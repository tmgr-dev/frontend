import { ownsOnlyDefaultWorkspace, runOnboardingBoot } from '../boot';
import type { OnboardingBootDeps } from '../boot';
import { emptyOnboarding } from '@/tours/store';

const deps = (over: Partial<OnboardingBootDeps> = {}): OnboardingBootDeps => ({
  state: () => emptyOnboarding(),
  localOnly: () => false,
  ready: jest.fn().mockResolvedValue(undefined),
  routeName: () => 'WorkspaceTasksList',
  redirect: jest.fn(),
  startSequence: jest.fn().mockResolvedValue(undefined),
  hasWorkspaceInvitation: () => false,
  user: () => ({ id: 1, created_at: '2999-01-01T00:00:00Z' }),
  workspaces: () => [{ id: 1, user_id: 1, is_default: true }],
  waitForSession: jest.fn().mockResolvedValue(undefined),
  ...over,
});

describe('ownsOnlyDefaultWorkspace', () => {
  it.each([
    [[{ user_id: 1 }], 1, true],
    [[{ user_id: '1' }], 1, true],
    [[{ user_id: 1 }, { user_id: 2 }], 1, false],
    [[], 1, false],
    [[{ user_id: 1 }, { user_id: 1 }], 1, false],
    [[{ user_id: 1 }], undefined, false],
  ])('%j for user %p is %p', (workspaces, userId, expected) => {
    expect(ownsOnlyDefaultWorkspace(workspaces as never, userId as never)).toBe(expected);
  });
});

describe('runOnboardingBoot', () => {
  it('sends a new account without an intent to the welcome screen', async () => {
    const d = deps();
    expect(await runOnboardingBoot(d)).toBe('redirected');
    expect(d.redirect).toHaveBeenCalledWith('WelcomeSetup');
  });

  it('waits for the router, the user and (invited unknown) the workspaces', async () => {
    const d = deps();
    await runOnboardingBoot(d);
    expect(d.ready).toHaveBeenCalled();
    expect(d.waitForSession).toHaveBeenCalledWith(true);
  });

  it('does not need workspaces when the server tells invited', async () => {
    const d = deps({ state: () => ({ ...emptyOnboarding(), invited: false }) });
    await runOnboardingBoot(d);
    expect(d.waitForSession).toHaveBeenCalledWith(false);
  });

  it('leaves an invited account alone', async () => {
    const d = deps({ state: () => ({ ...emptyOnboarding(), invited: true }) });
    expect(await runOnboardingBoot(d)).toBe('none');
    expect(d.redirect).not.toHaveBeenCalled();
  });

  it('leaves an account with a pending invitation alone', async () => {
    const d = deps({ hasWorkspaceInvitation: () => true });
    expect(await runOnboardingBoot(d)).toBe('none');
  });

  it('leaves a guest of another workspace alone until the server says invited', async () => {
    const d = deps({ workspaces: () => [{ id: 1, user_id: 1 }, { id: 2, user_id: 9 }] });
    expect(await runOnboardingBoot(d)).toBe('none');
  });

  it('does nothing without loaded state or on a malformed payload', async () => {
    expect(await runOnboardingBoot(deps({ state: () => null }))).toBe('none');
    expect(await runOnboardingBoot(deps({ state: () => ({ ...emptyOnboarding(), malformed: true }) }))).toBe('none');
  });

  it('leaves an old account alone', async () => {
    const d = deps({ user: () => ({ id: 1, created_at: '2020-01-01T00:00:00Z' }) });
    expect(await runOnboardingBoot(d)).toBe('none');
  });

  it('does not redirect from an excluded route', async () => {
    const d = deps({ routeName: () => 'WorkspaceInvitation' });
    expect(await runOnboardingBoot(d)).toBe('none');
  });

  describe('local mode', () => {
    const local = (over: Partial<OnboardingBootDeps> = {}) =>
      deps({ localOnly: () => true, state: () => ({ ...emptyOnboarding(), intent: 'human' }), ...over });

    it('never redirects and starts G1 then G2 on the first session', async () => {
      const d = local();
      expect(await runOnboardingBoot(d)).toBe('sequence');
      expect(d.redirect).not.toHaveBeenCalled();
      expect(d.startSequence).toHaveBeenCalledWith(['start.nav', 'start.first-task'], 'first_run');
    });

    it('does not repeat once a tour has state', async () => {
      const state = { ...emptyOnboarding(), intent: 'human', tours: { 'start.nav': { status: 'skipped', step_id: 's1', version: 1, generation: 0 } } };
      const d = local({ state: () => state as never });
      expect(await runOnboardingBoot(d)).toBe('none');
      expect(d.startSequence).not.toHaveBeenCalled();
    });

    it('does not start for a non-human intent', async () => {
      const d = local({ state: () => ({ ...emptyOnboarding(), intent: 'skipped' }) });
      expect(await runOnboardingBoot(d)).toBe('none');
    });
  });
});
