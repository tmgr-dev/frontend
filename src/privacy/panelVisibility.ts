import type { ConsentValue } from './consent';

export const CONSENT_PANEL_BUTTON_VARIANT = 'outline' as const;

export interface PanelState {
	analytics: ConsentValue;
	usage: ConsentValue;
	path: string;
	desktop: boolean;
}

export const showAnalyticsBlock = (state: PanelState): boolean =>
	!state.desktop && state.analytics === null;

export const showUsageBlock = (state: PanelState): boolean =>
	state.usage === null;

export const shouldShowConsentPanel = (state: PanelState): boolean =>
	!/^\/privacy(\/|$)/.test(state.path) &&
	(showAnalyticsBlock(state) || showUsageBlock(state));
