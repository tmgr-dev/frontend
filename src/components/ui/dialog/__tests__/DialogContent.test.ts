import { readFileSync } from 'fs';
import { join } from 'path';

describe('DialogContent', () => {
	it('lets grid children shrink below their content width', () => {
		const source = readFileSync(
			join(__dirname, '../DialogContent.vue'),
			'utf8',
		);
		expect(source).toContain('[&>*]:min-w-0');
	});
});
