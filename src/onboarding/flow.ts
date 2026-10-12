export type Intent = 'human' | 'agent' | 'skipped';

export type FlowStep = 'intent' | 'consent' | 'connect';

export const consentStepReady = (): boolean => false;

export function steps(_intent: Intent | null): FlowStep[] {
  return [];
}
