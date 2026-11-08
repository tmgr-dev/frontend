import { readFileSync } from 'fs';
import { join } from 'path';

const read = (name: string) =>
	readFileSync(join(__dirname, '..', name), 'utf8');

describe('DeletedSkills section', () => {
	const source = read('DeletedSkills.vue');

	it('is hidden when nothing is deleted and no error', () => {
		expect(source).toContain('v-if="error || items.length > 0"');
	});

	it.each([
		'deleted-skills',
		'deleted-skills-error',
		'deleted-skills-retry',
		'deleted-preview',
		'deleted-restore',
	])('has data-testid %s', (id) => {
		expect(source).toContain(`data-testid="${id}"`);
	});

	it('shows the purge date and the attachments note', () => {
		expect(source).toContain('settingsPages.deletedSkills.deletedOn');
		expect(source).toContain('settingsPages.deletedSkills.attachmentsNote');
	});

	it('reloads when the reload token changes', () => {
		expect(source).toMatch(/watch\(\s*\(\) => props\.reloadToken/);
	});

	it('disables Retry while loading', () => {
		expect(source.replace(/\s+/g, ' ')).toMatch(
			/data-testid="deleted-skills-retry"[^>]*:disabled="loading"|:disabled="loading"[^>]*data-testid="deleted-skills-retry"/,
		);
	});

	it('uses design tokens and no hard-coded colours', () => {
		expect(source).not.toMatch(/#[0-9a-fA-F]{3,6}\b/);
	});
});

describe('SkillsLibrary deleted section', () => {
	const source = read('SkillsLibrary.vue');

	it('renders the section, refreshes it after changes and forwards changed', () => {
		expect(source).toContain('<DeletedSkills');
		expect(source).toMatch(/:reload-token="deletedToken"/);
		expect(source).toMatch(/@changed="onRestored"/);
	});

	it('guards Edit and History for the reserved slug but not Delete', () => {
		const flat = source.replace(/\s+/g, ' ');
		expect(flat).toContain('isReservedSkillSlug(skill.slug)');
		expect(flat).toContain('RESERVED_SKILL_MESSAGE');
		expect(flat).toMatch(/@click="openHistory\(skill\)"/);
		expect(flat).toMatch(/@click="startEdit\(skill\)"/);
		expect(flat).toMatch(/@click="remove\(skill\)"/);
	});
});
