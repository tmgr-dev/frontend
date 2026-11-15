import { changedSettings } from '../changedSettings';

const baseline = [
	{ id: 1, key: 'timezone', value: 'Europe/Warsaw' },
	{ id: 2, key: 'theme', value: 'ocean' },
	{ id: 3, key: 'preferred_editor', value: null },
];

test('sends nothing when nothing changed', () => {
	expect(changedSettings(baseline, baseline)).toEqual([]);
});

test('sends only the entries whose value changed, as id and value', () => {
	const next = baseline.map((s) =>
		s.key === 'theme' ? { ...s, value: 'forest' } : s,
	);
	expect(changedSettings(next, baseline)).toEqual([{ id: 2, value: 'forest' }]);
});

test('treats 5 and "5" as the same value', () => {
	expect(
		changedSettings([{ id: 9, value: 5 }], [{ id: 9, value: '5' }]),
	).toEqual([]);
});

test('a blanked saved value is sent as an empty string, never null', () => {
	const next = baseline.map((s) =>
		s.key === 'timezone' ? { ...s, value: null } : s,
	);
	expect(changedSettings(next, baseline)).toEqual([{ id: 1, value: '' }]);
});

test('an unset value that stays unset is not sent', () => {
	expect(changedSettings([{ id: 3, value: '' }], baseline)).toEqual([]);
});

test('an entry missing from the baseline is sent when it has a value', () => {
	expect(changedSettings([{ id: 7, value: 'x' }], baseline)).toEqual([
		{ id: 7, value: 'x' },
	]);
});

test('entries without a numeric id are dropped', () => {
	expect(changedSettings([{ value: 'x' } as any], [])).toEqual([]);
});
