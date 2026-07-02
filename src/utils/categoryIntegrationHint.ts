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

const HINT_COPY: Record<
	CategoryIntegrationKind,
	{ title: string; message: string; ctaLabel: string }
> = {
	github: {
		title: 'No GitHub repository linked',
		message:
			'This category has no GitHub repository linked yet. Connect one in the category settings to track commits, branches and pull requests for its tasks.',
		ctaLabel: 'Open category settings',
	},
	cursor: {
		title: 'No Cursor agent configured',
		message:
			'This category has no Cursor agent configured yet. Add a Cursor API key in the category settings to launch agents on its tasks.',
		ctaLabel: 'Open category settings',
	},
};

export const getCategoryIntegrationHint = (
	kind: CategoryIntegrationKind,
	state: CategoryIntegrationState,
): CategoryIntegrationHint => {
	const configured = isCategoryIntegrationConfigured(kind, state);
	const copy = HINT_COPY[kind];
	return {
		show: !configured,
		kind,
		title: copy.title,
		message: copy.message,
		ctaLabel: copy.ctaLabel,
	};
};
