type SettingLike = { id?: unknown; value?: unknown };

const isEmpty = (value: unknown) =>
	value === null || value === undefined || value === '';

export const changedSettings = (
	settings: SettingLike[],
	baseline: SettingLike[],
): { id: number; value: unknown }[] => {
	const saved = new Map<number, unknown>();
	baseline.forEach((s) => {
		if (typeof s?.id === 'number') saved.set(s.id, s.value);
	});

	const result: { id: number; value: unknown }[] = [];
	settings.forEach((s) => {
		if (typeof s?.id !== 'number') return;
		const before = saved.get(s.id);
		if (isEmpty(s.value) && isEmpty(before)) return;
		if (isEmpty(s.value)) {
			result.push({ id: s.id, value: '' });
			return;
		}
		if (!isEmpty(before) && String(before) === String(s.value)) return;
		result.push({ id: s.id, value: s.value });
	});
	return result;
};
