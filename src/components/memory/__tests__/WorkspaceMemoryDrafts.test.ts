import { readFileSync } from 'fs';
import { join } from 'path';

const read = (name: string) =>
	readFileSync(join(__dirname, '..', name), 'utf8');

const page = readFileSync(
	join(__dirname, '../../../pages/WorkspaceMemory.vue'),
	'utf8',
);

test('noAddAllButton', () => {
	const sources = [
		page,
		read('SummarizeDraftCard.vue'),
		read('SummarizeWorkspaceModal.vue'),
	].join('\n');
	expect(sources).not.toMatch(/add\s*all|addAll|acceptAll/i);
	expect(sources).toContain('discardSummarizeRun');
});

test('summarize button is gated on edit rights, pages and cloud mode', () => {
	expect(page).toContain(
		'v-if="memory.can_edit && memory.pages_enabled && !isLocalOnly"',
	);
});

test('en and ru catalogs carry the same summarize keys', () => {
	const load = (lang: string) =>
		JSON.parse(
			readFileSync(
				join(__dirname, `../../../locales/${lang}/workspace.json`),
				'utf8',
			),
		).workspaceMemory.summarize as Record<string, unknown>;
	expect(Object.keys(load('ru')).sort()).toEqual(
		Object.keys(load('en')).sort(),
	);
});
