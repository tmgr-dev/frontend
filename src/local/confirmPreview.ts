import { confirmTask, snapshotOf, snapshotParts, snapshotSha } from './journal';
import type { LocalContext, LocalDb } from './types';

export interface PreviewComment {
	id: number;
	author_kind: string;
	author_name: string;
	message: string;
}

/** Everything an agent can read from the task, as plain text, plus the hash the confirmation is bound to. */
export interface ConfirmPreview {
	taskId: number;
	title: string;
	description: string;
	descriptionJson: string;
	checkpoints: string[];
	/** The complete stored checkpoints JSON, pretty-printed: exactly what the hash covers. */
	checkpointsText: string;
	comments: PreviewComment[];
	snapshotHash: string;
	shortHash: string;
}

const checkpointLines = (stored: string): string[] => {
	if (!stored) return [];
	try {
		const parsed = JSON.parse(stored);
		if (Array.isArray(parsed)) {
			return parsed.map((item) =>
				item && typeof item === 'object'
					? `${item.checked ? '[x]' : '[ ]'} ${String(item.description ?? '')}`
					: String(item),
			);
		}
	} catch {
		/* shown as stored */
	}
	return [stored];
};

const prettyJson = (stored: string): string => {
	if (!stored) return '';
	try {
		return JSON.stringify(JSON.parse(stored), null, 2);
	} catch {
		return stored;
	}
};

export const buildConfirmPreview = async (db: LocalDb, taskId: number): Promise<ConfirmPreview | null> => {
	const snapshot = await snapshotOf(db, taskId);
	if (!snapshot) return null;
	const authors = await db.select<{ id: number; author_kind: string | null; author_name: string | null }>(
		`SELECT id, author_kind, author_name FROM comments WHERE task_id = ? AND deleted_at IS NULL`,
		[taskId],
	);
	const byId = new Map(authors.map((row) => [Number(row.id), row]));
	const snapshotHash = await snapshotSha(await snapshotParts(snapshot));
	return {
		taskId,
		title: snapshot.title,
		description: snapshot.description,
		descriptionJson: snapshot.description_json,
		checkpoints: checkpointLines(snapshot.checkpoints),
		checkpointsText: prettyJson(snapshot.checkpoints),
		comments: snapshot.comments.map((comment) => ({
			id: comment.id,
			author_kind: byId.get(comment.id)?.author_kind ?? 'user',
			author_name: byId.get(comment.id)?.author_name ?? '',
			message: comment.message,
		})),
		snapshotHash,
		shortHash: snapshotHash.slice(0, 12),
	};
};

/** Confirms exactly the text that was shown: a task that changed since is refused, nothing is written. */
export const confirmShownTask = (ctx: LocalContext, preview: ConfirmPreview): Promise<string> =>
	confirmTask(ctx, preview.taskId, preview.snapshotHash);
