import type { BulkApprovalItem } from '@/actions/tmgr/agentRuns';
import {
	approvalMessage,
	contentSha256,
	reposSha256,
} from '@/utils/agentApproval';
import {
	coverageHashes,
	type CoverageTexts,
} from '@/utils/agentApprovalCoverage';

export interface BulkTask {
	id: number;
	title: string;
	description: string | null;
	workspaceId: number;
	categoryId: number;
	repositories?: string[] | null;
	coverage?: CoverageTexts;
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
			const repos = task.repositories
				? await reposSha256(task.repositories)
				: undefined;
			const covered = await coverageHashes(
				task.coverage ?? { persona: null, context: null },
			);
			const message = approvalMessage(
				input.host,
				input.userId,
				task.workspaceId,
				task.id,
				task.categoryId,
				sha,
				input.keyId,
				input.signedAt,
				repos,
				covered.persona_sha256,
				covered.context_sha256,
			);
			return {
				task_id: task.id,
				kind: 'confirm' as const,
				key_id: input.keyId,
				host: input.host,
				category_id: task.categoryId,
				content_sha256: sha,
				...(repos ? { repos_sha256: repos } : {}),
				...covered,
				signed_at: input.signedAt,
				signature: await input.sign(message),
			};
		}),
	);
