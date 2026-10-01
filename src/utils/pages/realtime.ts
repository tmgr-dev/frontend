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
