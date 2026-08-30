/**
 * Tracked seconds of a task including the slice currently running on its timer.
 * `start_time` is a Unix timestamp while running and 0/null when stopped.
 */
export function liveTaskTime(
	task: { common_time?: number | null; start_time?: number | null },
	nowSeconds: number,
): number {
	const tracked = task.common_time ?? 0;
	const start = task.start_time ?? 0;
	if (start <= 0) return tracked;
	return tracked + Math.max(0, nowSeconds - start);
}
