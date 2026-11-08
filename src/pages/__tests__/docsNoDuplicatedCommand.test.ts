import { readdirSync, readFileSync, statSync } from 'fs';
import { join, relative } from 'path';

const root = join(__dirname, '../../..');

const walk = (dir: string): string[] =>
	readdirSync(dir).flatMap((name) => {
		const full = join(dir, name);
		return statSync(full).isDirectory() ? walk(full) : [full];
	});

const docs = walk(join(root, 'src/docs'))
	.filter((f) => f.endsWith('.md'))
	.map((f) => ({ file: relative(root, f), text: readFileSync(f, 'utf8') }));

const stripFrontMatter = (text: string) =>
	text.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, '');

// Paragraph immediately followed (optionally by one blank line) by a fenced
// block whose whole content equals one of the paragraph's inline codes.
const duplicates = (text: string): string[] => {
	const lines = stripFrontMatter(text).split(/\r?\n/);
	const found: string[] = [];
	for (let i = 0; i < lines.length; i++) {
		if (!/^\s*```/.test(lines[i])) continue;
		let end = i + 1;
		while (end < lines.length && !/^\s*```/.test(lines[end])) end++;
		const body = lines
			.slice(i + 1, end)
			.join('\n')
			.trim();
		let p = i - 1;
		if (p >= 0 && lines[p].trim() === '') p--;
		const para: string[] = [];
		while (p >= 0 && lines[p].trim() !== '' && !/^\s*```/.test(lines[p])) {
			para.unshift(lines[p--]);
		}
		const codes = [...para.join(' ').matchAll(/`([^`]+)`/g)].map((m) =>
			m[1].trim(),
		);
		if (body && codes.includes(body)) found.push(body);
		i = end;
	}
	return found;
};

describe('docs do not repeat the command shown in the next code block', () => {
	it.each(docs.map((d) => [d.file, d.text]))('%s', (_file, text) => {
		expect(duplicates(text)).toEqual([]);
	});
});
