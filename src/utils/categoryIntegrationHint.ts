import { t } from '@/i18n';

export type CategoryIntegrationKind = 'github' | 'cursor';

export interface CategoryIntegrationState {
	hasRepository: boolean;
	cursorConfigured: boolean;
}

export interface CategoryIntegrationHint {
	show: boolean;
	kind: CategoryIntegrationKind;
	title: string;
	message: string;
	ctaLabel: string;
}

export const isCategoryIntegrationConfigured = (
	kind: CategoryIntegrationKind,
	state: CategoryIntegrationState,
): boolean =>
	kind === 'github' ? !!state.hasRepository : !!state.cursorConfigured;

export const getCategoryIntegrationHint = (
	kind: CategoryIntegrationKind,
	state: CategoryIntegrationState,
): CategoryIntegrationHint => {
	const configured = isCategoryIntegrationConfigured(kind, state);
	return {
		show: !configured,
		kind,
		title: t(`board.helpers.integrationHint.${kind}.title`),
		message: t(`board.helpers.integrationHint.${kind}.message`),
		ctaLabel: t('board.helpers.integrationHint.openCategorySettings'),
	};
};
