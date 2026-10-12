import { changedFields, type PageDraft } from './conflict';

export interface PageUpdatedEvent {
	page?: { id: number; version: number };
}

export const shouldShowUpdateBanner = (input: {
	pageId: number;
	loadedVersion: number;
	ownVersions: Set<number>;
	event: PageUpdatedEvent | undefined;
}): number | null => {
	const page = input.event?.page;
	if (!page || page.id !== input.pageId) return null;
	if (page.version <= input.loadedVersion) return null;
	if (input.ownVersions.has(page.version)) return null;
	return page.version;
};

export type RefreshAction = 'ignore' | 'adopt' | 'conflict';

export const refreshAction = (input: {
	serverVersion: number;
	loadedVersion: number;
	ownVersions: Set<number>;
	form: PageDraft;
	saved: PageDraft;
}): RefreshAction => {
	if (
		input.serverVersion <= input.loadedVersion ||
		input.ownVersions.has(input.serverVersion)
	) {
		return 'ignore';
	}
	return Object.keys(changedFields(input.form, input.saved)).length
		? 'conflict'
		: 'adopt';
};
