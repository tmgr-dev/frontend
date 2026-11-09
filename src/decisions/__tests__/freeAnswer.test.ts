import { readFileSync } from 'fs';
import { join } from 'path';
import { parse } from 'vue/compiler-sfc';

const read = (name: string): string =>
	readFileSync(join(__dirname, '..', name), 'utf8');

const cardPane = read('components/CardPane.vue');
const template = parse(cardPane).descriptor.template?.content ?? '';
const css = read('decisions.css');

const rule = (selector: string): string => {
	const start = css.indexOf(`\n${selector} {`);
	expect(start).toBeGreaterThanOrEqual(0);
	return css.slice(start, css.indexOf('}', start));
};

describe('CardPane free answer', () => {
	const freeTag =
		template.match(/<textarea\s+ref="free"[\s\S]*?><\/textarea>/)?.[0] ?? '';

	it('routes keys through onFreeKey, which uses the shared submit rule', () => {
		expect(freeTag).toContain('@keydown="onFreeKey"');
		expect(cardPane).toMatch(
			/onFreeKey\(event: KeyboardEvent\) \{\s*if \(isSubmitKey\(event\) && this\.freeText\.trim\(\)\)/,
		);
	});

	it('starts at one row and grows with its text', () => {
		expect(freeTag).toContain('rows="1"');
		expect(cardPane).toMatch(
			/freeText\(value: string\) \{[\s\S]*?void this\.\$nextTick\(\(\) => this\.sizeFree\(\)\);/,
		);
		expect(cardPane).toMatch(/mounted\(\) \{\s*this\.sizeFree\(\);/);
	});

	it('sizes the free textarea with a 240px cap', () => {
		expect(cardPane).toMatch(
			/sizeFree\(\) \{\s*autosizeTextarea\(\s*this\.\$refs\.free as HTMLTextAreaElement \| undefined,\s*FREE_MAX_HEIGHT,\s*\);/,
		);
		expect(cardPane).toMatch(/const FREE_MAX_HEIGHT = 240;/);
	});
});

describe('decisions layout', () => {
	it('lets the free answer start at 44px and grow up to 240px', () => {
		const free = rule('.free');
		expect(free).toContain('min-height: 44px;');
		expect(free).toContain('max-height: 240px;');
		expect(free).toContain('resize: none;');
		expect(free).not.toMatch(/\n\theight:/);
	});

	it('lets the description grow to half the window height', () => {
		expect(rule('.md')).toContain('max-height: 50vh;');
	});

	it('gives the card column more width than the right pane', () => {
		expect(rule('.canvas')).toContain(
			'grid-template-columns: minmax(360px, 3fr) minmax(320px, 2fr);',
		);
	});
});
