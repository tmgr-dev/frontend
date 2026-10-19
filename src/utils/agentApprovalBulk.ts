import type { BulkApprovalItem } from '@/actions/tmgr/agentRuns';
import { approvalMessage, contentSha256 } from '@/utils/agentApproval';

export interface BulkTask {
	id: number;
	title: string;
	description: string | null;
	workspaceId: number;
	categoryId: number;
}

export const buildBulkItems = async (input: {
	tasks: BulkTask[];
	host: string;
	userId: number;
	keyId: string;
	signedAt: number;
	sign: (message: string) => Promise<string>;
}): Promise<BulkApprovalItem[]> =>
	Promise.all(
		input.tasks.map(async (task) => {
			const sha = await contentSha256(task.title, task.description);
			const message = approvalMessage(
				input.host,
				input.userId,
				task.workspaceId,
				task.id,
				task.categoryId,
				sha,
				input.keyId,
				input.signedAt,
			);
			return {
				task_id: task.id,
				kind: 'confirm' as const,
				key_id: input.keyId,
				host: input.host,
				category_id: task.categoryId,
				content_sha256: sha,
				signed_at: input.signedAt,
				signature: await input.sign(message),
			};
		}),
	);
