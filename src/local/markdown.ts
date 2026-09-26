/** Markdown export of local workspaces: one file per task plus a README index. Pure functions. */

export interface ExportTask {
	id: number;
	title: string;
	description: string | null;
	description_json: any;
	status: { name: string; type: string } | null;
	category: { title: string; code: string | null } | null;
	category_tasks_sequence_id: number | null;
	priority: string | null;
	common_time: number;
	approximately_time: number;
	checkpoints: { description?: string; checked?: boolean }[];
	created_at: string;
	updated_at: string;
	comments: { message: string; created_at: string; author: string }[];
	files: { name: string; file_path: string }[];
}

export interface ExportFile {
	path: string;
	content: string;
}

export const taskKey = (task: Pick<ExportTask, 'id' | 'category' | 'category_tasks_sequence_id'>) =>
	task.category?.code && task.category_tasks_sequence_id
		? `${task.category.code}-${task.category_tasks_sequence_id}`
		: `T-${task.id}`;

const slug = (text: string) =>
	text
		.normalize('NFKD')
		.toLowerCase()
		.replace(/[^a-z0-9а-яё]+/gi, '-')
		.replace(/^-+|-+$/g, '')
		.slice(0, 60) || 'task';

export const taskFileName = (task: ExportTask) => `${taskKey(task)}-${slug(task.title)}.md`;

export const formatDuration = (seconds: number) => {
	const total = Math.max(0, Math.floor(seconds || 0));
	const h = Math.floor(total / 3600);
	const m = Math.floor((total % 3600) / 60);
	return h ? `${h}h ${m}m` : `${m}m`;
};

const yamlString = (value: string) => JSON.stringify(value);

const stripHtml = (html: string) =>
	html
		.replace(/<br\s*\/?>/gi, '\n')
		.replace(/<b>|<\/b>|<strong>|<\/strong>/gi, '**')
		.replace(/<i>|<\/i>|<em>|<\/em>/gi, '_')
		.replace(/<code[^>]*>|<\/code>/gi, '`')
		.replace(/<a [^>]*href="([^"]+)"[^>]*>(.*?)<\/a>/gi, '[$2]($1)')
		.replace(/<[^>]+>/g, '')
		.replace(/&nbsp;/g, ' ')
		.replace(/&lt;/g, '<')
		.replace(/&gt;/g, '>')
		.replace(/&amp;/g, '&');

const listItems = (items: any[], ordered: boolean, depth = 0): string[] =>
	items.flatMap((item, index) => {
		const text = typeof item === 'string' ? item : (item?.content ?? item?.text ?? '');
		const marker = ordered ? `${index + 1}.` : '-';
		const line = `${'  '.repeat(depth)}${marker} ${stripHtml(String(text))}`;
		const children = Array.isArray(item?.items) ? listItems(item.items, ordered, depth + 1) : [];
		return [line, ...children];
	});

/** Editor.js blocks → Markdown for the block types the task editor offers. */
export const editorJsToMarkdown = (doc: any): string => {
	const blocks: any[] = Array.isArray(doc?.blocks) ? doc.blocks : [];
	return blocks
		.map((block) => {
			const data = block?.data ?? {};
			switch (block?.type) {
				case 'header':
					return `${'#'.repeat(Math.min(6, Math.max(2, Number(data.level) || 2)))} ${stripHtml(data.text ?? '')}`;
				case 'list':
					return listItems(data.items ?? [], data.style === 'ordered').join('\n');
				case 'checklist':
					return (data.items ?? [])
						.map((item: any) => `- [${item.checked ? 'x' : ' '}] ${stripHtml(item.text ?? '')}`)
						.join('\n');
				case 'code':
					return '```\n' + (data.code ?? '') + '\n```';
				case 'quote':
					return `> ${stripHtml(data.text ?? '')}`;
				case 'delimiter':
					return '---';
				case 'paragraph':
				default:
					return stripHtml(data.text ?? '');
			}
		})
		.filter((text) => text.trim() !== '')
		.join('\n\n');
};

const descriptionOf = (task: ExportTask) =>
	task.description_json?.blocks?.length
		? editorJsToMarkdown(task.description_json)
		: (task.description ?? '').trim();

/** `filesBase` is the path from the exported file to the workspace `files/` folder. */
export const taskToMarkdown = (task: ExportTask, filesBase: string): string => {
	const front = [
		'---',
		`key: ${taskKey(task)}`,
		`title: ${yamlString(task.title)}`,
		task.status ? `status: ${yamlString(task.status.name)}` : null,
		task.category ? `category: ${yamlString(task.category.title)}` : null,
		task.priority ? `priority: ${task.priority}` : null,
		`time_spent: ${formatDuration(task.common_time)}`,
		task.approximately_time ? `estimate: ${formatDuration(task.approximately_time)}` : null,
		`created: ${task.created_at}`,
		`updated: ${task.updated_at}`,
		'---',
	].filter(Boolean);
	const sections = [`# ${taskKey(task)}: ${task.title}`];
	const description = descriptionOf(task);
	if (description) sections.push(description);
	if (task.checkpoints?.length) {
		sections.push(
			'## Checkpoints\n\n' +
				task.checkpoints
					.map((c) => `- [${c.checked ? 'x' : ' '}] ${c.description ?? ''}`.trimEnd())
					.join('\n'),
		);
	}
	if (task.files.length) {
		sections.push(
			'## Attachments\n\n' +
				task.files.map((f) => `- [${f.name}](${filesBase}/${encodeURI(f.file_path)})`).join('\n'),
		);
	}
	if (task.comments.length) {
		sections.push(
			'## Comments\n\n' +
				task.comments.map((c) => `**${c.author}** · ${c.created_at}\n\n${c.message}`).join('\n\n---\n\n'),
		);
	}
	return `${front.join('\n')}\n\n${sections.join('\n\n')}\n`;
};

/** README index of the export, tasks grouped by status in board order. */
export const workspaceIndex = (
	name: string,
	statuses: { name: string }[],
	tasks: ExportTask[],
	exportedAt: string,
): string => {
	const lines = [`# ${name}`, '', `Exported ${exportedAt} · ${tasks.length} tasks`, ''];
	const groups = [...statuses.map((s) => s.name), 'No status'];
	for (const group of groups) {
		const inGroup = tasks.filter((t) => (t.status?.name ?? 'No status') === group);
		if (!inGroup.length) continue;
		lines.push(`## ${group}`, '');
		inGroup.forEach((t) => lines.push(`- [${taskKey(t)}: ${t.title}](tasks/${encodeURI(taskFileName(t))})`));
		lines.push('');
	}
	return lines.join('\n');
};

export const exportWorkspaceFiles = (
	name: string,
	statuses: { name: string }[],
	tasks: ExportTask[],
	exportedAt: string,
): ExportFile[] => [
	{ path: 'README.md', content: workspaceIndex(name, statuses, tasks, exportedAt) },
	...tasks.map((task) => ({
		path: `tasks/${taskFileName(task)}`,
		// exports/<stamp>/tasks/x.md → ../../../files
		content: taskToMarkdown(task, '../../../files'),
	})),
];
