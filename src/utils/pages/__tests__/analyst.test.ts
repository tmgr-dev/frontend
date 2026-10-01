import {
	ANALYST_SETTING_KEY,
	analystOptions,
	readSettingValue,
} from '../analyst';

const grant = (id: string, name: string, extra = {}) => ({
	persona: { id, name },
	...extra,
});

describe('analystOptions', () => {
	it('lists granted personas and hides blocked or archived ones', () => {
		const options = analystOptions(
			[
				grant('a', 'Ann'),
				grant('b', 'Bob', { blocked: true }),
				{ persona: { id: 'c', name: 'Cy', archived: true } },
			],
			null,
		);
		expect(options).toEqual([{ value: 'a', label: 'Ann' }]);
	});

	it('keeps the currently selected persona even if it is gone or blocked', () => {
		expect(analystOptions([grant('b', 'Bob', { blocked: true })], 'b')).toEqual(
			[{ value: 'b', label: 'Bob' }],
		);
		expect(analystOptions([], 'zzz')).toEqual([{ value: 'zzz', label: 'zzz' }]);
	});
});

describe('readSettingValue', () => {
	it('reads from a map or a list of key/value rows', () => {
		expect(
			readSettingValue({ [ANALYST_SETTING_KEY]: 'a' }, ANALYST_SETTING_KEY),
		).toBe('a');
		expect(
			readSettingValue(
				[{ key: ANALYST_SETTING_KEY, value: 'b' }],
				ANALYST_SETTING_KEY,
			),
		).toBe('b');
		expect(readSettingValue(null, ANALYST_SETTING_KEY)).toBeNull();
		expect(readSettingValue({}, ANALYST_SETTING_KEY)).toBeNull();
	});
});
