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

describe('proposed by', () => {
	const en = JSON.parse(
		readFileSync(join(__dirname, '../../../locales/en/workspace.json'), 'utf8'),
	).workspaceMemory;

	test('en catalog formats the proposer name as plain text', () => {
		expect(en.proposedBy).toBe('Proposed by {name}');
		expect(en.proposedBy.replace('{name}', '<img src=x>')).toBe(
			'Proposed by <img src=x>',
		);
	});

	test('page shows proposer only when set, via text interpolation', () => {
		expect(page).toMatch(
			/<p\s+v-if="card\.proposed_by"[\s\S]*?\$t\('workspace\.workspaceMemory\.proposedBy',\s*\{\s*name: card\.proposed_by,?\s*\}\)/,
		);
		expect(page).not.toContain('v-html');
	});
});
