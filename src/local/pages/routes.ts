import type { LocalRouter } from '../router';
import { TASK_SELECT, taskJson } from '../serialize';
import { syncTaskMentions } from './mentions';
import {
	appendToPage,
	attachPageFile,
	createPage,
	deletePage,
	getPage,
	listPages,
	movePage,
	pageBacklinks,
	pageFiles,
	pageVersion,
	pageVersions,
	pagesForTask,
	pinPage,
	restorePage,
	restorePageVersion,
	searchPages,
	setPageSection,
	taskFromSelection,
	treePages,
	trashPages,
	updatePage,
	workspaceContext,
} from './service';

const parentOf = (raw: string | null): number | null => {
	if (raw === null || raw === '' || raw === 'null') return null;
	const id = Number(raw);
	return Number.isFinite(id) ? id : null;
};

export const addPageRoutes = (router: LocalRouter): LocalRouter =>
	router
		.add('GET', 'workspaces/context', async ({ ctx }) => ({ markdown: await workspaceContext(ctx) }))
		.add('GET', 'pages', ({ ctx, query }) => listPages(ctx, parentOf(query.get('parent_id')), query.get('type')))
		.add('GET', 'pages/tree', ({ ctx }) => treePages(ctx))
		.add('GET', 'pages/trash', ({ ctx }) => trashPages(ctx))
		.add('GET', 'pages/search', ({ ctx, query }) =>
			searchPages(ctx, query.get('q'), query.get('type'), query.get('limit')),
		)
		.add('POST', 'pages', ({ ctx, body }) => createPage(ctx, body), 201)
		.add('GET', 'pages/:id', ({ ctx, params }) => getPage(ctx, params.id))
		.add('PATCH', 'pages/:id', ({ ctx, params, body }) => updatePage(ctx, params.id, body))
		.add('POST', 'pages/:id/append', ({ ctx, params, body }) => appendToPage(ctx, params.id, body))
		.add('PUT', 'pages/:id/sections/:sectionId', ({ ctx, params, body }) =>
			setPageSection(ctx, params.id, params.sectionId, body),
		)
		.add('POST', 'pages/:id/move', ({ ctx, params, body }) => movePage(ctx, params.id, body))
		.add('POST', 'pages/:id/pin', ({ ctx, params }) => pinPage(ctx, params.id, true))
		.add('POST', 'pages/:id/unpin', ({ ctx, params }) => pinPage(ctx, params.id, false))
		.add('DELETE', 'pages/:id', ({ ctx, params }) => deletePage(ctx, params.id))
		.add('POST', 'pages/:id/restore', ({ ctx, params }) => restorePage(ctx, params.id))
		.add('GET', 'pages/:id/versions', ({ ctx, params }) => pageVersions(ctx, params.id))
		.add('GET', 'pages/:id/versions/:version(\\d+)', ({ ctx, params }) =>
			pageVersion(ctx, params.id, Number(params.version)),
		)
		.add('POST', 'pages/:id/versions/:version(\\d+)/restore', ({ ctx, params }) =>
			restorePageVersion(ctx, params.id, Number(params.version)),
		)
		.add('GET', 'pages/:id/backlinks', ({ ctx, params }) => pageBacklinks(ctx, params.id))
		.add('GET', 'pages/:id/files', ({ ctx, params }) => pageFiles(ctx, params.id))
		.add('POST', 'pages/:id/files', ({ ctx, params, body }) => attachPageFile(ctx, params.id, body), 201)
		.add(
			'POST',
			'pages/:id/task-from-selection',
			async ({ ctx, params, body }) => {
				const { taskId, key, page } = await taskFromSelection(ctx, params.id, body);
				await syncTaskMentions(ctx, taskId);
				const [row] = await ctx.db.select(`${TASK_SELECT} WHERE t.id = ?`, [taskId]);
				return { task: { ...taskJson(row, ctx), key, url: null }, page };
			},
			201,
		)
		.add('POST', 'pages/:id/follow', () => null, 204)
		.add('DELETE', 'pages/:id/follow', () => null, 204)
		.add('GET', 'tasks/:id(\\d+)/pages', ({ ctx, params }) => pagesForTask(ctx, Number(params.id)));
