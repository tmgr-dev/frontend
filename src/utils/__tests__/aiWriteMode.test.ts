import { AI_WRITE_MODE_KEY, parseAiWriteMode } from '../aiWriteMode';
import { readSettingValue } from '../pages/analyst';

describe('parseAiWriteMode', () => {
	it('defaults to read only when missing or unknown', () => {
		expect(parseAiWriteMode(null)).toBe('read_only');
		expect(parseAiWriteMode(undefined)).toBe('read_only');
		expect(parseAiWriteMode('anything')).toBe('read_only');
	});

	it('accepts write with confirmation', () => {
		expect(parseAiWriteMode('write_with_confirmation')).toBe(
			'write_with_confirmation',
		);
	});

	it('reads the value from a settings map or list', () => {
		expect(
			parseAiWriteMode(
				readSettingValue(
					{ [AI_WRITE_MODE_KEY]: 'write_with_confirmation' },
					AI_WRITE_MODE_KEY,
				),
			),
		).toBe('write_with_confirmation');
		expect(
			parseAiWriteMode(
				readSettingValue(
					[{ key: AI_WRITE_MODE_KEY, value: 'read_only' }],
					AI_WRITE_MODE_KEY,
				),
			),
		).toBe('read_only');
	});
});
