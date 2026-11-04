import {
	isHttpUrl,
	linkLabel,
	recommendedOption,
	sourceLabel,
} from '../format';
import { makeCard } from './fixtures';

describe('format', () => {
	it('only treats http and https as openable links', () => {
		expect(isHttpUrl('https://example.com/x')).toBe(true);
		expect(isHttpUrl('javascript:alert(1)')).toBe(false);
		expect(isHttpUrl('file:///etc/passwd')).toBe(false);
		expect(isHttpUrl('TM-5')).toBe(false);
	});

	it('shortens links to a chip label', () => {
		expect(linkLabel('https://github.com/o/r/pull/714')).toBe('PR #714');
		expect(linkLabel('https://github.com/o/r/issues/9')).toBe('#9');
		expect(linkLabel('https://example.com/a/b')).toBe('example.com/a/b');
		expect(linkLabel('TM-780')).toBe('TM-780');
		expect(linkLabel(`https://example.com/${'x'.repeat(60)}`)).toHaveLength(30);
	});

	it('finds the recommended option and labels the source', () => {
		const card = makeCard({
			options: [
				{ id: 'a', label: 'A', recommended: false },
				{ id: 'b', label: 'B', recommended: true },
			],
			source: { agent: 'codex', title: 'Review' },
		});
		expect(recommendedOption(card)?.id).toBe('b');
		expect(recommendedOption(makeCard())).toBeNull();
		expect(sourceLabel(card)).toBe('codex · Review');
	});
});
