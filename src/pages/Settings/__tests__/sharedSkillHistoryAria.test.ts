import { readFileSync } from 'fs';
import { join } from 'path';

describe('SharedSkillHistory', () => {
	it('marks the selected version for assistive tech', () => {
		const source = readFileSync(
			join(__dirname, '../SharedSkillHistory.vue'),
			'utf8',
		);
		expect(source).toMatch(/:aria-current="v\.version === selected/);
	});
});
