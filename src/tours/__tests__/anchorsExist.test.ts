import { readdirSync, readFileSync, statSync } from 'fs';
import { join } from 'path';
import { TOURS } from '../catalog';

const NOT_YET_IN_VUE = new Set<string>();

const walk = (dir: string): string[] =>
	readdirSync(dir).flatMap((name) => {
		const path = join(dir, name);
		return statSync(path).isDirectory() ? walk(path) : [path];
	});

const anchors = () =>
	new Set(
		TOURS.flatMap((t) => t.steps.flatMap((s) => [s.anchor, s.mobileAnchor]))
			.filter((a): a is string => !!a)
			.filter((a) => !NOT_YET_IN_VUE.has(a)),
	);

describe('every tour anchor exists as data-tour in a .vue file', () => {
	const source = walk(join(__dirname, '../..'))
		.filter((f) => f.endsWith('.vue'))
		.map((f) => readFileSync(f, 'utf8'))
		.join('\n');

	it.each([...anchors()])('%s', (anchor) => {
		expect(source).toMatch(new RegExp(`data-tour=["']${anchor.replace(/\./g, '\\.')}["']|data-tour="[^"]*${anchor.replace(/\./g, '\\.')}`));
	});
});

it('anchor list is collected', () => {
	expect(anchors().size).toBeGreaterThan(10);
});
