import type { ConsentValue } from './consent';

export const CONSENT_PANEL_BUTTON_VARIANT = 'outline' as const;

export const shouldShowConsentPanel = (state: {
	analytics: ConsentValue;
	path: string;
	desktop: boolean;
}): boolean =>
	!state.desktop &&
	state.analytics === null &&
	!/^\/privacy(\/|$)/.test(state.path);
