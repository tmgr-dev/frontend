import { readFileSync } from 'fs';
import { join } from 'path';
import { bundleCodeboxHighlight } from '../codeboxHighlight';

describe('bundleCodeboxHighlight', () => {
	afterEach(() => {
		delete (globalThis as { window?: unknown }).window;
	});

	it('replaces the CDN script injection with the bundled highlighter', () => {
		const added: unknown[] = [];
		(globalThis as { window?: unknown }).window = {};
		(globalThis as { document?: unknown }).document = {
			querySelector: () => null,
			createElement: () => {
				added.push(1);
				return {};
			},
		};
		class Tool {
			_injectHighlightJSScriptElement() {
				added.push('cdn');
			}
		}
		bundleCodeboxHighlight(Tool);
		new Tool()._injectHighlightJSScriptElement();
		expect(added).toEqual([]);
		const hljs = (globalThis as unknown as { window: { hljs: unknown } })
			.window.hljs as { highlightBlock: unknown };
		expect(typeof hljs.highlightBlock).toBe('function');
	});
});

describe('desktop CSP', () => {
	it('does not allow telegram.org or jsdelivr', () => {
		const conf = readFileSync(
			join(__dirname, '../../../src-tauri/tauri.conf.json'),
			'utf8',
		);
		expect(conf).not.toMatch(/telegram\.org|jsdelivr/);
	});
});
