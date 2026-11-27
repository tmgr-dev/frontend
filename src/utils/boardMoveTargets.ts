export type MoveTargetStatus = {
	id: number | string;
	type?: string | null;
	pivot?: { order?: number | null } | null;
};

const boardOrder = (status: MoveTargetStatus) =>
	Number(status.pivot?.order ?? 0) || 0;

export const moveTargetStatuses = <T extends MoveTargetStatus>(
	statuses: T[] | null | undefined,
	currentStatusId: number | string | null | undefined,
): T[] =>
	(statuses || [])
		.filter(
			(status) =>
				status.type !== 'archived' &&
				Number(status.id) !== Number(currentStatusId),
		)
		.sort((a, b) => boardOrder(a) - boardOrder(b));
