import {
	isHttpUrl,
	linkLabel,
	linkSections,
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
		expect(linkLabel('https://github.com/o/r/releases/tag/v1.4.2')).toBe(
			'v1.4.2',
		);
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

describe('link sections', () => {
	it('shows old cards with only task and pr', () => {
		const sections = linkSections({
			task: 'https://tmgr.dev/ws/tasks/5',
			pr: 'https://github.com/o/r/pull/7',
		});
		expect(sections.map((s) => s.title)).toEqual(['PRs', 'Task']);
		expect(sections[0].entries[0].label).toBe('PR #7');
	});

	it('merges items, groups them and dedups by url', () => {
		const sections = linkSections({
			pr: 'https://github.com/o/r/pull/7',
			items: [
				{ kind: 'pr', url: 'https://github.com/o/r/pull/7', label: 'Main PR' },
				{ kind: 'commit', url: 'https://github.com/o/r/commit/abcdef1' },
				{ kind: 'tag', url: 'https://github.com/o/r/releases/tag/v1' },
				{ kind: 'url', url: 'https://example.com/doc' },
			],
		});
		expect(sections.map((s) => s.title)).toEqual([
			'PRs',
			'Commits',
			'Tags',
			'Other',
		]);
		expect(sections[0].entries).toHaveLength(1);
		expect(sections[0].entries[0].label).toBe('Main PR');
		expect(sections[3].entries[0].label).toBe('example.com/doc');
	});

	it('keeps a non-url task as text', () => {
		const [section] = linkSections({ task: 'TM-780' });
		expect(section.entries[0]).toMatchObject({
			label: 'TM-780',
			url: 'TM-780',
		});
	});

	it('is empty without links', () => {
		expect(linkSections({})).toEqual([]);
	});
});
