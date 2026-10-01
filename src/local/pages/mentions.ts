import type { LocalContext } from '../types';
import { PROMISES_SECTION } from './markdown';
import { refreshManagedSection, taskKeyOf } from './service';

const PAGE_LINK = /tmgr:\\?\/\\?\/page\\?\/(\d+)/g;
const KEY_PREFIX = /^\s*[A-Za-z][A-Za-z0-9]*-\d+\s*:\s*/;
const MAX_PAGES_PER_TASK = 100;
const PROMISES_SUMMARY = 'Обещания обновлены';

const mentionedIds = (texts: (string | null | undefined)[]): number[] => {
	const ids = new Set<number>();
	for (const text of texts) {
		if (!text) continue;
		for (const m of text.matchAll(PAGE_LINK)) {
			if (ids.size >= MAX_PAGES_PER_TASK) break;
			ids.add(Number(m[1]));
		}
	}
	return [...ids];
};

const marks = (ids: number[]) => ids.map(() => '?').join(',');

const promiseLine = (row: any): string => {
	const key = taskKeyOf(row.title, row.code, row.seq, row.id);
	const title = String(row.title ?? '')
		.replace(KEY_PREFIX, '')
		.replace(/\s+/g, ' ')
		.replace(/</g, '&lt;')
		.trim();
	let line = `- [${key}](tmgr://task/${row.id}) — ${title}`;
	if (row.expired_at) line += ` · до ${String(row.expired_at).slice(0, 10)}`;
	if (row.status_name && String(row.status_name).trim()) line += ` · ${String(row.status_name).trim()}`;
	return line;
};

const rebuildPromises = async (ctx: LocalContext, pageId: number) => {
	const rows = await ctx.db.select<any>(
		`SELECT t.id, t.title, t.expired_at, t.category_tasks_sequence_id AS seq, c.code, s.name AS status_name
		 FROM task_page_mentions m
		 JOIN tasks t ON t.id = m.task_id AND t.deleted_at IS NULL
		 LEFT JOIN categories c ON c.id = t.project_category_id
		 LEFT JOIN statuses s ON s.id = t.status_id
		 WHERE m.page_id = ? AND (s.type IS NULL OR s.type NOT IN ('completed', 'archived'))
		 ORDER BY t.expired_at IS NULL, t.expired_at, t.id`,
		[pageId],
	);
	await refreshManagedSection(ctx, pageId, PROMISES_SECTION, rows.map(promiseLine).join('\n'), PROMISES_SUMMARY);
};

/** Re-reads a task and its comments, updates task_page_mentions and rebuilds the promises of the person pages it touches. */
export const syncTaskMentions = async (ctx: LocalContext, taskId: number): Promise<void> => {
	try {
		const [task] = await ctx.db.select<any>(
			`SELECT description, description_json, deleted_at FROM tasks WHERE id = ?`,
			[taskId],
		);
		if (!task) return;
		const stored = (
			await ctx.db.select<{ page_id: number }>(`SELECT page_id FROM task_page_mentions WHERE task_id = ?`, [taskId])
		).map((r) => r.page_id);
		let wanted: number[] = [];
		if (task.deleted_at === null) {
			const comments = await ctx.db.select<{ message: string }>(
				`SELECT message FROM comments WHERE task_id = ? AND deleted_at IS NULL`,
				[taskId],
			);
			const candidates = mentionedIds([task.description, task.description_json, ...comments.map((c) => c.message)]);
			if (candidates.length) {
				wanted = (
					await ctx.db.select<{ id: number }>(
						`SELECT id FROM pages WHERE deleted_at IS NULL AND id IN (${marks(candidates)})`,
						candidates,
					)
				).map((r) => r.id);
			}
		}
		if (!stored.length && !wanted.length) return;
		for (const id of stored.filter((id) => !wanted.includes(id))) {
			await ctx.db.execute(`DELETE FROM task_page_mentions WHERE task_id = ? AND page_id = ?`, [taskId, id]);
		}
		for (const id of wanted.filter((id) => !stored.includes(id))) {
			await ctx.db.execute(`INSERT OR IGNORE INTO task_page_mentions (task_id, page_id) VALUES (?, ?)`, [taskId, id]);
		}
		const affected = [...new Set([...stored, ...wanted])];
		const people = await ctx.db.select<{ id: number }>(
			`SELECT id FROM pages WHERE deleted_at IS NULL AND type = 'person' AND id IN (${marks(affected)})`,
			affected,
		);
		for (const person of people) await rebuildPromises(ctx, person.id);
	} catch (error) {
		console.error('[pages] could not refresh task mentions', error);
	}
};

export const mentionedPeople = async (ctx: LocalContext, taskId: number) =>
	ctx.db.select<{ page_id: number; slug: string; title: string }>(
		`SELECT p.id AS page_id, p.slug, p.title FROM task_page_mentions m JOIN pages p ON p.id = m.page_id
		 WHERE m.task_id = ? AND p.type = 'person' AND p.deleted_at IS NULL ORDER BY p.title, p.id`,
		[taskId],
	);
