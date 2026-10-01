import { domainEvents } from '@/utils/domainEvents';
import type { LocalContext } from '../types';
import { authorJson } from './serialize';
import type { PageRow } from './serialize';

export type PageEventType = 'page.created' | 'page.updated' | 'page.deleted' | 'page.restored' | 'page.moved';

const actorLabel = (ctx: LocalContext): string | undefined =>
	ctx.actor?.kind === 'persona' || ctx.actor?.kind === 'plugin' ? `${ctx.actor.kind}:${ctx.actor.id}` : undefined;

/** Same payload the cloud realtime channel carries; the actor tag lets live updates skip the UI's own writes. */
export const emitPageEvent = (
	ctx: LocalContext,
	type: PageEventType,
	row: PageRow,
	summary: string | null,
	linkedTaskIds: number[],
) => {
	try {
		const actor = actorLabel(ctx);
		domainEvents.emit({
			type,
			workspaceId: ctx.workspace.id,
			page: {
				id: row.id,
				slug: row.slug,
				title: row.title,
				type: row.type,
				parent_id: row.parent_id,
				version: row.version,
				updated_by: authorJson(ctx, row.updated_by_kind, row.updated_by_ref, new Map()),
				summary,
				linked_task_ids: linkedTaskIds,
			},
			...(actor ? { actor } : {}),
		});
	} catch (error) {
		console.error('[pages] failed to publish an event', error);
	}
};
