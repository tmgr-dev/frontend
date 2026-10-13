import type { RunnerStatus } from './api';

export const BYPASS_PHRASE = 'I AGREE';

export const isBypassPhrase = (text: string): boolean => text === BYPASS_PHRASE;

export const bypassAvailable = (status: RunnerStatus | null): boolean =>
	status?.state === 'ready' && status.caps.includes('runs.bypass');
