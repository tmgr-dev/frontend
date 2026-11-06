import { readFileSync } from 'fs';
import { join } from 'path';

const source = readFileSync(
	join(__dirname, '../PersonaSharedSkills.vue'),
	'utf8',
);

describe('PersonaSharedSkills guards', () => {
	it('disables the version select while a pin change is pending', () => {
		expect(source).toMatch(/:disabled="pendingSlugs\.has\(skill\.slug\)"/);
		expect(source).toMatch(/pendingSlugs\.value\.has\(skill\.slug\)\) return/);
	});

	it('applies only the latest version list response per slug', () => {
		expect(source).toMatch(/versionRequests\.get\(skill\.slug\) !== requestId/);
	});
});
