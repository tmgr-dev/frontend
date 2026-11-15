import { renderSegment, type DraftItem } from './extraction';
import { formatTimecode, parseTranscript } from './transcriptParser';

export interface StoredItem extends DraftItem {
	key: string;
	selected: boolean;
}

export interface MeetingRecord {
	id: string;
	title: string;
	date: string;
	importedAt: number;
	transcript: string;
	participantsInformed: boolean;
	items: StoredItem[];
	created: Record<string, { taskId: number; commented: boolean }>;
	pageId: number | null;
}

export interface CreateApi {
	createTask(payload: Record<string, unknown>): Promise<{ id: number }>;
	createComment(taskId: number, payload: { message: string }): Promise<unknown>;
	createPage(payload: Record<string, unknown>): Promise<{ id: number }>;
}

export interface CreateTarget {
	workspaceId: number;
	statusId: number;
	categoryId?: number | null;
}

export interface CreateResult {
	record: MeetingRecord;
	tasksCreated: number;
	commentsCreated: number;
	pageCreated: boolean;
	errors: string[];
}

const timecode = (item: DraftItem): string | null =>
	item.startMs === null ? null : formatTimecode(item.startMs);

export const taskDescription = (
	item: DraftItem,
	meetingTitle: string,
): string => {
	const at = timecode(item);
	const source = `Source: meeting "${meetingTitle}"${at ? ` at ${at}` : ''}`;
	return item.description ? `${item.description}\n\n${source}` : source;
};

export const commentMessage = (item: DraftItem): string => {
	const at = timecode(item);
	const lines = [
		`AI draft${at ? ` · ${at}` : ''}`,
		`Why this is a task: ${item.why || 'not stated'}`,
		`Quote: "${item.quote}"`,
	];
	if (item.assigneeHint) lines.push(`Said by: ${item.assigneeHint}`);
	return lines.join('\n\n');
};

export const pageBody = (
	record: MeetingRecord,
	taskTitles: string[],
): string => {
	const { segments } = parseTranscript(record.transcript);
	const parts: string[] = [];
	if (taskTitles.length) {
		parts.push(
			`## Action items\n\n${taskTitles.map((t) => `- ${t}`).join('\n')}`,
		);
	}
	parts.push(`## Transcript\n\n${segments.map(renderSegment).join('\n\n')}`);
	return parts.join('\n\n');
};

const messageOf = (error: unknown): string =>
	error instanceof Error ? error.message : String(error);

export const createOutputs = async (
	api: CreateApi,
	input: MeetingRecord,
	target: CreateTarget,
	options: { withPage: boolean; save: (record: MeetingRecord) => void },
): Promise<CreateResult> => {
	const record: MeetingRecord = structuredClone(input);
	const errors: string[] = [];
	let tasksCreated = 0;
	let commentsCreated = 0;
	let pageCreated = false;
	const save = () => options.save(structuredClone(record));

	for (const item of record.items) {
		if (!item.selected) continue;
		let entry = record.created[item.key];
		if (!entry) {
			try {
				const task = await api.createTask({
					title: item.title,
					description: taskDescription(item, record.title),
					workspace_id: target.workspaceId,
					status_id: target.statusId,
					status: 'created',
					is_daily_routine: false,
					...(target.categoryId
						? { project_category_id: target.categoryId }
						: {}),
				});
				entry = { taskId: task.id, commented: false };
				record.created[item.key] = entry;
				tasksCreated += 1;
				save();
			} catch (error) {
				errors.push(`${item.title}: ${messageOf(error)}`);
				continue;
			}
		}
		if (!entry.commented) {
			try {
				await api.createComment(entry.taskId, {
					message: commentMessage(item),
				});
				entry.commented = true;
				commentsCreated += 1;
				save();
			} catch (error) {
				errors.push(`${item.title}: ${messageOf(error)}`);
			}
		}
	}

	const selected = record.items.filter((item) => item.selected);
	const complete = selected.every(
		(item) => record.created[item.key]?.commented,
	);
	if (options.withPage && !record.pageId && complete) {
		try {
			const done = selected.filter((item) => record.created[item.key]);
			const participants = [
				...new Set(
					parseTranscript(record.transcript)
						.segments.map((s) => s.speaker)
						.filter((s): s is string => Boolean(s)),
				),
			];
			const page = await api.createPage({
				title: `Meeting: ${record.title}, ${record.date}`,
				type: 'meeting',
				body: pageBody(
					record,
					done.map((item) => item.title),
				),
				properties: {
					date: record.date,
					participants,
					related_tasks: done.map((item) => record.created[item.key].taskId),
				},
			});
			record.pageId = page.id;
			pageCreated = true;
			save();
		} catch (error) {
			errors.push(`Page: ${messageOf(error)}`);
		}
	}

	return { record, tasksCreated, commentsCreated, pageCreated, errors };
};
