import { EXCLUDED_ROUTES, shouldOfferIntent } from '../redirect';
import type { RedirectContext } from '../redirect';
import { emptyOnboarding } from '@/tours/store';

const SINCE = Date.parse('2026-10-13T00:00:00Z');

const ctx = (over: Partial<RedirectContext> = {}): RedirectContext => ({
  state: emptyOnboarding(),
  hasWorkspaceInvitation: false,
  localOnly: false,
  mainWindow: true,
  routeName: 'WorkspaceTasksList',
  userCreatedAt: '2026-10-14T09:00:00Z',
  onboardingSince: SINCE,
  ownsOnlyDefaultWorkspace: true,
  ...over,
});

describe('shouldOfferIntent', () => {
  it('offers to a new account with no intent and no tasks', () => {
    expect(shouldOfferIntent(ctx())).toBe(true);
  });

  it.each([
    ['null state', { state: null }],
    ['undefined state', { state: undefined }],
    ['array payload', { state: [] as never }],
    ['string payload', { state: 'x' as never }],
    ['malformed flag', { state: { ...emptyOnboarding(), malformed: true } }],
    ['intent human', { state: { ...emptyOnboarding(), intent: 'human' } }],
    ['intent skipped', { state: { ...emptyOnboarding(), intent: 'skipped' } }],
    ['intent undefined', { state: { ...emptyOnboarding(), intent: undefined as never } }],
    ['invited true', { state: { ...emptyOnboarding(), invited: true } }],
    ['pending invitation', { hasWorkspaceInvitation: true }],
    ['local only', { localOnly: true }],
    ['detached window', { mainWindow: false }],
    ['unknown route', { routeName: null }],
    ['old account', { userCreatedAt: '2026-10-01T09:00:00Z' }],
    ['unparsable created_at with task', { userCreatedAt: 'nope', state: { ...emptyOnboarding(), facts: { has_task: true } } }],
    ['null created_at', { userCreatedAt: null }],
    ['empty created_at', { userCreatedAt: '' }],
    ['unparsable created_at', { userCreatedAt: 'nope' }],
    ['has task', { state: { ...emptyOnboarding(), facts: { has_task: true } } }],
    ['guest of another workspace (invited unknown)', { ownsOnlyDefaultWorkspace: false }],
  ])('refuses: %s', (_name, over) => {
    expect(shouldOfferIntent(ctx(over as Partial<RedirectContext>))).toBe(false);
  });

  it.each(EXCLUDED_ROUTES.concat(['Login', 'Register', 'SocialiteProxy', 'DesktopAuthReturn']))(
    'refuses on route %s',
    (routeName) => {
      expect(shouldOfferIntent(ctx({ routeName }))).toBe(false);
    },
  );

  it('trusts invited=false over the workspace fallback', () => {
    expect(
      shouldOfferIntent(
        ctx({ state: { ...emptyOnboarding(), invited: false }, ownsOnlyDefaultWorkspace: false }),
      ),
    ).toBe(true);
  });

  it('accepts an account created exactly at the boundary', () => {
    expect(shouldOfferIntent(ctx({ userCreatedAt: '2026-10-13T00:00:00Z' }))).toBe(true);
  });
});
