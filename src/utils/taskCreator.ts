/** TM-615: the member who created a task, or null when it can't be named (local task, left the workspace). */
export function taskCreator<T extends { id: number }>(
	members: T[] | null | undefined,
	userId: number | string | null | undefined,
	isLocal: boolean,
): T | null {
	if (isLocal || !userId) {
		return null;
	}

	return members?.find((member) => member.id === Number(userId)) ?? null;
}
