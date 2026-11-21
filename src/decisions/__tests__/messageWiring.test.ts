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
			/<span[^>]*v-if="tab === 'diff' && currentHasChanges && totals"[\s\S]*?\+\{\{ totals\.added \}\}[\s\S]*?−\{\{ totals\.removed \}\}/,
		);
	});

	it('opens links from the chat and the diff thread', () => {
		const source = template('App.vue');
		expect(source).toMatch(/<DiffPane[\s\S]*?@open-link="openLink"[\s\S]*?\/>/);
		expect(source).toMatch(/<ChatPane[\s\S]*?@open-link="openLink"[\s\S]*?\/>/);
	});
});

describe('CardPane.vue fact links', () => {
	it('opens only http(s) fact links and shows anything else as plain text', () => {
		const source = template('CardPane.vue');
		expect(source).toMatch(
			/<button[^>]*v-if="fact\.link && isHttpUrl\(fact\.link\)"[\s\S]*?@click="openLink\(fact\.link\)"/,
		);
		expect(source).toMatch(
			/<span v-else-if="fact\.link" class="chip">\{\{\s*linkLabel\(fact\.link\)\s*\}\}<\/span>/,
		);
	});
});

describe('action panel wiring', () => {
	const panel = template('ActionPanel.vue');

	it('replaces the option buttons for action cards', () => {
		const card = template('CardPane.vue');
		expect(card).toMatch(/<ActionPanel[\s\S]*?v-if="card\.action"/);
		expect(card).toContain('<template v-else-if="!card.action">');
	});

	it('disables Reject until a reason is typed', () => {
		expect(panel).toMatch(/data-reject[\s\S]*?:disabled="!reason\.trim\(\)"/);
	});

	it('edits only editable fields and offers Reset', () => {
		expect(panel).toContain('v-if="row.editable && open"');
		expect(panel).toContain('data-reset');
		expect(panel).toContain('data-edited');
	});

	it('renders owner text as plain text, never html', () => {
		expect(panel).not.toContain('v-html');
	});

	it('shows the PR guarantee note and run states', () => {
		expect(panel).toContain("action.kind === 'pr'");
		expect(panel).toContain('decisions.action.run.');
		expect(panel).toContain('data-rejected');
	});
});
