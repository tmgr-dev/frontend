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

export const ONBOARDING_SINCE = Date.UTC(2026, 9, 12);

export const EXCLUDED_ROUTES: readonly string[] = [
	'WorkspaceInvitation',
	'WelcomeSetup',
	'WelcomeSetupAgent',
	'Privacy',
	'VerifyEmail',
	'Login',
	'Register',
	'SocialiteProxy',
	'DesktopAuthReturn',
	'DesktopAuthTelegram',
	'ForgetPassword',
	'NewPassword',
];

export function shouldOfferIntent(ctx: RedirectContext): boolean {
	const state = ctx.state;
	if (!state || typeof state !== 'object' || Array.isArray(state)) return false;
	if (state.malformed === true || state.intent !== null) return false;
	if (state.invited === true) return false;
	if (state.invited === undefined && !ctx.ownsOnlyDefaultWorkspace)
		return false;
	if (ctx.hasWorkspaceInvitation || ctx.localOnly || !ctx.mainWindow)
		return false;
	if (!ctx.routeName || EXCLUDED_ROUTES.includes(ctx.routeName)) return false;
	if (state.facts?.has_task) return false;
	const createdAt = ctx.userCreatedAt ? Date.parse(ctx.userCreatedAt) : NaN;
	return Number.isFinite(createdAt) && createdAt >= ctx.onboardingSince;
}
