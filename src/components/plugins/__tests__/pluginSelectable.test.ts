import { readFileSync } from 'fs';
import { join } from 'path';

const read = (...parts: string[]) =>
	readFileSync(join(__dirname, '..', '..', '..', ...parts), 'utf8');

describe('plugin content is selectable in the desktop app', () => {
	it('marks the plugin page content', () => {
		expect(read('pages', 'PluginPage.vue')).toMatch(
			/<div v-else-if="tree" data-selectable>\s*<PluginView/,
		);
	});

	it('marks the plugin task sections', () => {
		expect(read('components', 'plugins', 'PluginTaskSections.vue')).toMatch(
			/<div v-if="trees\[section\.key\]" data-selectable>\s*<PluginView/,
		);
	});

	it('keeps buttons, tabs, segmented options and icons non-selectable', () => {
		const css = read('assets', 'styles', '_extensions.scss');
		expect(css).toMatch(
			/\[data-selectable\]\s*:is\(button, \[role='button'\], \[role='tab'\], \[role='radio'\], svg\)\s*\{[^}]*user-select: none/,
		);
	});
});
