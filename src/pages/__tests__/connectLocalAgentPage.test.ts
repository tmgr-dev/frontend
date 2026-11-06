import { readFileSync } from 'fs';
import { join } from 'path';

const raw = readFileSync(
	join(__dirname, '../desktop/ConnectLocalAgent.vue'),
	'utf8',
);
const src = raw.replace(/\s+/g, ' ');

describe('ConnectLocalAgent.vue', () => {
	it('keeps the tour anchors and the screen texts', () => {
		expect(src).toContain('data-tour="connect-local.agents"');
		expect(src).toContain('data-tour="connect-local.install"');
		expect(src).toContain(
			'Agent connection on this screen works for local workspaces in the desktop app.',
		);
		expect(src).toContain(
			'Create your own workspace first, then connect your agent.',
		);
	});

	it('uses defineComponent and no script setup', () => {
		expect(src).toContain('defineComponent(');
		expect(src).not.toMatch(/<script[^>]*\bsetup\b/);
	});

	it('gives dark variants to coloured text', () => {
		const coloured =
			src.match(
				/class="[^"]*\b(?:text|bg|border)-(?:red|amber|emerald)-\d+[^"]*"/g,
			) ?? [];
		expect(coloured.length).toBeGreaterThan(0);
		for (const cls of coloured) expect(cls).toMatch(/dark:/);
	});
});
