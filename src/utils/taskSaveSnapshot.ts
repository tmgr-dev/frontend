/** Apply server normalization only to fields untouched since this request began. */
export function mergeSavedTask<T extends { id?: number }>(
	current: T,
	sent: T,
	saved: T,
): T {
	if (current.id !== sent.id) return current;
	const merged = { ...current };
	for (const key of Object.keys(saved) as (keyof T)[]) {
		if (JSON.stringify(current[key]) === JSON.stringify(sent[key]))
			merged[key] = saved[key];
	}
	return merged;
}
