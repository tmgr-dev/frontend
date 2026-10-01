export const shouldReloadTaskPages = (
	event: { type: string; page: any } | null | undefined,
	taskId: number,
): boolean => {
	if (!event || !event.type.startsWith('page.')) return false;
	const ids = event.page?.linked_task_ids;
	return Array.isArray(ids) && ids.includes(taskId);
};
