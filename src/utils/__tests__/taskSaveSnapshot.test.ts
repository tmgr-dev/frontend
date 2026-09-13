import { mergeSavedTask } from '../taskSaveSnapshot';
it('does not apply A response to task B', () => {
	const current = { id: 2, title: 'B' };
	expect(
		mergeSavedTask(current, { id: 1, title: 'A' }, { id: 1, title: 'saved A' }),
	).toBe(current);
});
it('preserves every edited field including structured editor content', () => {
	const sent = { id: 1, title: 'A', content: { blocks: ['old'] }, status: 1 };
	const current = { ...sent, content: { blocks: ['new'] } };
	expect(mergeSavedTask(current, sent, { ...sent, status: 2 })).toEqual({
		...current,
		status: 2,
	});
});
