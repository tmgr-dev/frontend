import { readFileSync } from 'fs';
import { join } from 'path';

describe('SkillsLibrary history panel', () => {
	it('remounts the history when the personas revision changes', () => {
		const source = readFileSync(
			join(__dirname, '../SkillsLibrary.vue'),
			'utf8',
		);
		expect(source).toMatch(/:key="`[^`]*\$\{revision\}`"/);
	});
});
