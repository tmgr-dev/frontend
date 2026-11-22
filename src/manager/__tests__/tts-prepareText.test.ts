import { chunkText, prepareSpeech } from '../tts/prepareText';

describe('prepareSpeech', () => {
	it('strips markdown and keeps the readable text', () => {
		const r = prepareSpeech(
			'# Plan\n\n**Bold** and *it* and ~~old~~.\n\n- first item\n2. second\n\nSee [the docs](https://example.com/x).',
		);
		expect(r.lang).toBe('en');
		expect(r.chunks.join(' ')).toBe(
			'Plan Bold and it and old. first item second See the docs.',
		);
	});

	it('drops images and reduces tables to plain cells', () => {
		const r = prepareSpeech(
			'![chart](https://x.test/y.png) Result\n\n| Name | Qty |\n| --- | --- |\n| apple | 3 |',
		);
		expect(r.chunks.join(' ')).toBe('Result Name, Qty apple, 3');
	});

	it('drops URLs and emoji and collapses whitespace', () => {
		const r = prepareSpeech('Docs at https://example.com/a/b   now 🎉 ✅  ok');
		expect(r.chunks.join(' ')).toBe('Docs at now ok');
	});

	it('keeps inline code text', () => {
		expect(prepareSpeech('Use `npm test` now.').chunks.join(' ')).toBe(
			'Use npm test now.',
		);
	});

	it('replaces a fenced block with the English phrase', () => {
		const r = prepareSpeech('Run this:\n```ts\nconst a = 1;\n```\nDone.');
		expect(r.lang).toBe('en');
		expect(r.chunks.join(' ')).toBe('Run this: code block omitted. Done.');
	});

	it('replaces a fenced block with the Russian phrase when the text is Russian', () => {
		const r = prepareSpeech('Смотри:\n```\nx\n```\nготово.');
		expect(r.lang).toBe('ru');
		expect(r.chunks.join(' ')).toBe('Смотри: блок кода пропущен. готово.');
	});

	it('returns no chunks for an empty body', () => {
		expect(prepareSpeech('  \n ').chunks).toEqual([]);
	});

	it('detects Russian when Cyrillic is at least 30 percent of letters', () => {
		expect(prepareSpeech('Привет, как дела? Всё хорошо.').lang).toBe('ru');
		expect(prepareSpeech('abcd жи').lang).toBe('ru');
	});

	it('detects English below the 30 percent Cyrillic threshold', () => {
		expect(prepareSpeech('abcdefg жи').lang).toBe('en');
		expect(
			prepareSpeech(
				'The deploy finished successfully today with no errors, мы ок',
			).lang,
		).toBe('en');
	});
});

describe('chunkText', () => {
	const sentence = `${'a'.repeat(120)}.`;

	it('keeps short text in one chunk', () => {
		expect(chunkText('One. Two.', 300)).toEqual(['One. Two.']);
	});

	it('packs whole sentences up to the limit and splits between them', () => {
		expect(chunkText(`${sentence} ${sentence} ${sentence}`, 300)).toEqual([
			`${sentence} ${sentence}`,
			sentence,
		]);
	});

	it('hard-splits an overlong sentence at spaces without cutting words', () => {
		const words = Array.from({ length: 120 }, (_, i) => `w${i}`);
		const text = `${words.join(' ')}.`;
		const chunks = chunkText(text, 300);
		expect(chunks.length).toBeGreaterThan(1);
		expect(chunks.every((c) => c.length > 0 && c.length <= 300)).toBe(true);
		expect(chunks.every((c) => c === c.trim())).toBe(true);
		expect(chunks.join(' ')).toBe(text);
	});

	it('hard-splits at the passed max, not the default', () => {
		const text = Array.from({ length: 30 }, (_, i) => `w${i}`).join(' ');
		const chunks = chunkText(text, 50);
		expect(chunks.length).toBeGreaterThan(1);
		expect(chunks.every((c) => c.length <= 50)).toBe(true);
		expect(chunks.join(' ')).toBe(text);
	});
});
