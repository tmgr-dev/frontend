import type { DomainEvent } from '@/utils/domainEvents';

type PageEvent = Extract<DomainEvent, { page: unknown }>;

const SECTION =
	/<!--\s*tmgr:section\s+id="([^"]+)"[^>]*-->([\s\S]*?)<!--\s*\/tmgr:section\s*-->/g;

export const sectionContents = (body: unknown): Map<string, string> => {
	const sections = new Map<string, string>();
	if (typeof body !== 'string') return sections;
	for (const match of body.matchAll(SECTION))
		sections.set(match[1], match[2].trim());
	return sections;
};

const MAX_TRACKED = 500;

/**
 * `changedSections` needs the page as it was: the last body seen per page tells which sections differ.
 * A page seen for the first time reports every section it has (never fewer than changed); realtime events
 * from shared workspaces carry no body, so they report none.
 */
export const createPageEventMapper = () => {
	const seen = new Map<string, Map<string, string>>();
	return (event: PageEvent): Record<string, unknown> => {
		const page = event.page as Record<string, any>;
		const key = `${event.workspaceId}:${event.pageId}`;
		let changedSections: string[] = [];
		if (typeof page.body === 'string') {
			const current = sectionContents(page.body);
			const previous = seen.get(key);
			if (event.changedSections) changedSections = event.changedSections;
			else if (previous)
				changedSections = [
					...new Set([...current.keys(), ...previous.keys()]),
				].filter((id) => current.get(id) !== previous.get(id));
			else changedSections = [...current.keys()];
			seen.delete(key);
			seen.set(key, current);
			if (seen.size > MAX_TRACKED)
				seen.delete(seen.keys().next().value as string);
		} else if (event.changedSections) {
			changedSections = event.changedSections;
		}
		const author =
			event.type === 'page.created'
				? page.author ?? page.updated_by
				: page.updated_by ?? page.author;
		return {
			type: event.type,
			workspaceId: event.workspaceId,
			pageId: event.pageId,
			slug: page.slug ?? null,
			title: page.title ?? null,
			parentId: page.parent_id ?? null,
			version: page.version ?? null,
			author: author
				? { kind: author.kind ?? null, id: author.id ?? null }
				: null,
			changedSections,
		};
	};
};
