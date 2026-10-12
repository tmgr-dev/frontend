export const STARTERS_EXPECTED_KEY = 'tmgr:onboarding:starters-expected';

export interface StarterVisibility {
  intent: string | null;
  hasUser: boolean;
  removed: boolean;
  boardTasks: boolean;
  boardHasStarter: boolean;
  expected: boolean;
}

export function showRemoveStarters(ctx: StarterVisibility): boolean {
  if (!ctx.hasUser || ctx.removed) return false;
  if (ctx.intent !== 'human' && ctx.intent !== 'agent') return false;
  return ctx.boardTasks ? ctx.boardHasStarter : ctx.expected;
}
