export type Intent = 'human' | 'agent' | 'skipped';

export type FlowStep = 'intent' | 'consent' | 'connect';

export const consentStepReady = (): boolean => false;

export function steps(intent: Intent | null): FlowStep[] {
  const path: FlowStep[] = ['intent'];
  if (intent === null || intent === 'skipped') return path;
  if (consentStepReady()) path.push('consent');
  if (intent === 'agent') path.push('connect');
  return path;
}
