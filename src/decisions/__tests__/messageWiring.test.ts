import { readFileSync } from 'fs';
import { join } from 'path';
import { parse } from 'vue/compiler-sfc';

const read = (name: string): string =>
	readFileSync(join(__dirname, '..', 'components', name), 'utf8');

const template = (name: string): string =>
	parse(read(name)).descriptor.template?.content ?? '';

describe.each(['ChatPane.vue', 'DiffPane.vue'])('%s', (name) => {
	const source = template(name);

	it('renders only agent messages as markdown', () => {
		expect(source).toMatch(
			/v-if="isAgent\(message\)"[\s\S]*?v-html="render\(message\.text\)"/,
		);
		expect(source).toMatch(
			/<template v-else>\{\{ message\.text \}\}<\/template>/,
		);
	});

	it('never feeds owner text into v-html', () => {
		const htmlBindings = source.match(/v-html="[^"]*"/g) ?? [];
		expect(htmlBindings).toEqual(['v-html="render(message.text)"']);
	});

	it('forwards link clicks upward', () => {
		expect(source).toContain('@click="onMarkdownClick"');
		expect(read(name)).toMatch(/emits: \[[^\]]*'open-link'/);
	});
});

describe('App.vue', () => {
	it('shows the diff totals in the tab header', () => {
		expect(template('App.vue')).toMatch(
			/<span[^>]*v-if="tab === 'diff' && totals"[\s\S]*?\+\{\{ totals\.added \}\}[\s\S]*?−\{\{ totals\.removed \}\}/,
		);
	});

	it('opens links from the chat and the diff thread', () => {
		const source = template('App.vue');
		expect(source).toMatch(/<DiffPane[\s\S]*?@open-link="openLink"[\s\S]*?\/>/);
		expect(source).toMatch(/<ChatPane[\s\S]*?@open-link="openLink"[\s\S]*?\/>/);
	});
});
