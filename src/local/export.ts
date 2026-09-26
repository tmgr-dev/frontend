import { exportWorkspaceFiles, taskFileName, taskToMarkdown, type ExportFile, type ExportTask } from './markdown';
import { parseJson } from './serialize';
import type { LocalDb } from './types';

const loadTasks = async (db: LocalDb, author: string, taskId?: number): Promise<ExportTask[]> => {
	const tasks = await db.select<any>(
		`SELECT t.*, s.name AS s_name, s.type AS s_type, c.title AS c_title, c.code AS c_code
		 FROM tasks t
		 LEFT JOIN statuses s ON s.id = t.status_id
		 LEFT JOIN categories c ON c.id = t.project_category_id
		 WHERE t.deleted_at IS NULL${taskId ? ' AND t.id = ?' : ''}
		 ORDER BY t.id`,
		taskId ? [taskId] : [],
	);
	const comments = await db.select<any>(
		`SELECT task_id, message, created_at FROM comments WHERE deleted_at IS NULL ORDER BY id`,
	);
	const files = await db.select<any>(`SELECT task_id, name, file_path FROM files ORDER BY id`);
	return tasks.map((t) => ({
		id: t.id,
		title: t.title,
		description: t.description,
		description_json: parseJson(t.description_json, null),
		status: t.s_name ? { name: t.s_name, type: t.s_type } : null,
		category: t.c_title ? { title: t.c_title, code: t.c_code } : null,
		category_tasks_sequence_id: t.category_tasks_sequence_id,
		priority: t.priority,
		common_time: Number(t.common_time ?? 0),
		approximately_time: Number(t.approximately_time ?? 0),
		checkpoints: parseJson(t.checkpoints, []),
		created_at: t.created_at,
		updated_at: t.updated_at,
		comments: comments
			.filter((c) => c.task_id === t.id)
			.map((c) => ({ message: c.message, created_at: c.created_at, author })),
		files: files.filter((f) => f.task_id === t.id).map((f) => ({ name: f.name, file_path: f.file_path })),
	}));
};

export const workspaceExport = async (
	db: LocalDb,
	name: string,
	author: string,
	exportedAt: string,
): Promise<ExportFile[]> => {
	const statuses = await db.select<{ name: string }>(
		`SELECT name FROM statuses ORDER BY CASE WHEN type = 'archived' THEN 1 ELSE 0 END, sort_order, id`,
	);
	return exportWorkspaceFiles(name, statuses, await loadTasks(db, author), exportedAt);
};

/** One task, written to exports/tasks/ (so attachments are two levels up). */
export const taskExport = async (db: LocalDb, taskId: number, author: string): Promise<ExportFile | null> => {
	const [task] = await loadTasks(db, author, taskId);
	return task ? { path: taskFileName(task), content: taskToMarkdown(task, '../../files') } : null;
};
