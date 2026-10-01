import { domainEvents } from '@/utils/domainEvents';
import type { LocalContext } from '../types';
import type { PageRow } from './serialize';
import { authorJson } from './serialize';

export type PageEventType =
	| 'page.created'
	| 'page.updated'
	| 'page.deleted'
	| 'page.restored'
	| 'page.moved';

const actorLabel = (ctx: LocalContext): string | undefined =>
	ctx.actor?.kind === 'persona' || ctx.actor?.kind === 'plugin'
		? `${ctx.actor.kind}:${ctx.actor.id}`
		: undefined;

/** Same payload the cloud realtime channel carries; the actor tag lets live updates skip the UI's own writes. */
export const emitPageEvent = (
	ctx: LocalContext,
	type: PageEventType,
	row: PageRow,
	summary: string | null,
	linkedTaskIds: number[],
	actorOverride?: string,
	changedSections?: string[],
) => {
	try {
		const actor = actorLabel(ctx) ?? actorOverride;
		domainEvents.emit({
			type,
			workspaceId: ctx.workspace.id,
			pageId: row.id,
			page: {
				id: row.id,
				slug: row.slug,
				title: row.title,
				type: row.type,
				parent_id: row.parent_id,
				version: row.version,
				updated_by: authorJson(
					ctx,
					row.updated_by_kind,
					row.updated_by_ref,
					new Map(),
				),
				summary,
				linked_task_ids: linkedTaskIds,
			},
			...(changedSections ? { changedSections } : {}),
			...(actor ? { actor } : {}),
		});
	} catch (error) {
		console.error('[pages] failed to publish an event', error);
	}
};
