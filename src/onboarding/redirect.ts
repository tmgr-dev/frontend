import type { OnboardingState } from '@/tours/types';

export interface RedirectContext {
  state: OnboardingState | null | undefined;
  hasWorkspaceInvitation: boolean;
  localOnly: boolean;
  mainWindow: boolean;
  routeName: string | null;
  userCreatedAt: string | null;
  onboardingSince: number;
  ownsOnlyDefaultWorkspace: boolean;
}

export const ONBOARDING_SINCE = 0;

export const EXCLUDED_ROUTES: readonly string[] = [
  'WorkspaceInvitation',
  'WelcomeSetup',
  'WelcomeSetupAgent',
  'Privacy',
  'VerifyEmail',
];

export function shouldOfferIntent(_ctx: RedirectContext): boolean {
  return false;
}
