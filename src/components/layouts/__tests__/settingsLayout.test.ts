import { readFileSync } from 'fs';
import { join } from 'path';

const read = (path: string) =>
	readFileSync(join(__dirname, '..', '..', '..', path), 'utf8');

describe('theme picker grid', () => {
	const picker = read('components/general/ThemePicker.vue');

	it('never shows more than three cards per row', () => {
		expect(picker).toContain('sm:grid-cols-2 lg:grid-cols-3');
		expect(picker).not.toMatch(/grid-cols-[4-9]/);
	});

	it('lets the card header and the mode row wrap instead of clipping', () => {
		expect(picker).toContain('mb-2 flex flex-wrap items-center');
		expect(picker).toContain('mt-2 flex flex-wrap items-center');
	});
});

describe('stacked page header', () => {
	const header = read('components/layouts/PageHeader.vue');

	it('keeps the side-by-side layout only when not stacked', () => {
		expect(header).toContain('stacked: { type: Boolean, default: false }');
		expect(header).toContain(
			":class=\"stacked ? '' : 'sm:flex-row sm:items-start sm:justify-between'\"",
		);
		expect(header).not.toMatch(/\sclass="[^"]*sm:flex-row/);
	});

	it.each(['Workspaces', 'Personas'])(
		'Settings %s stacks its title and actions',
		(page) => {
			const source = read(`pages/Settings/${page}.vue`);
			expect(source).toMatch(/<PageHeader stacked /);
		},
	);
});
