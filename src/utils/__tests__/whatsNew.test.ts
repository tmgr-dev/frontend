import { readFileSync } from 'fs';
import { join } from 'path';
import {
	compareVersions,
	parseChangelog,
	recentSections,
	whatsNewSections,
} from '../whatsNew';

const SAMPLE = `# Changelog

Header text that is not a section.

## 0.9.8 — 2026-09-29

- First bullet that is long and
  wrapped onto a second line
  and a third.
- Second bullet.

## 0.9.7 — 2026-09-28

- Only bullet.

## 0.9.10 — 2026-10-01

- Newest by number, but not first-numbered lexically.
`;

const sections = parseChangelog(SAMPLE);
const versions = (list: { version: string }[]) => list.map((s) => s.version);

describe('compareVersions', () => {
	it('compares numerically', () => {
		expect(compareVersions('0.9.10', '0.9.9')).toBe(1);
		expect(compareVersions('0.9.9', '0.9.10')).toBe(-1);
		expect(compareVersions('1.0.0', '0.99.99')).toBe(1);
	});
	it('treats equal versions as equal', () => {
		expect(compareVersions('0.9.8', '0.9.8')).toBe(0);
	});
	it('tolerates a v prefix and missing parts', () => {
		expect(compareVersions('v0.9.8', '0.9.8')).toBe(0);
		expect(compareVersions('1.2', '1.2.0')).toBe(0);
		expect(compareVersions('1', '1.0.1')).toBe(-1);
	});
});

describe('parseChangelog', () => {
	it('extracts sections with version and date, ignoring the header', () => {
		expect(sections.map(({ version, date }) => ({ version, date }))).toEqual([
			{ version: '0.9.8', date: '2026-09-29' },
			{ version: '0.9.7', date: '2026-09-28' },
			{ version: '0.9.10', date: '2026-10-01' },
		]);
		expect(JSON.stringify(sections)).not.toContain('Header text');
	});
	it('joins hard-wrapped continuation lines onto their bullet', () => {
		expect(sections[0].body).toBe(
			'- First bullet that is long and wrapped onto a second line and a third.\n- Second bullet.',
		);
	});
	it('keeps blank lines between paragraphs', () => {
		const [section] = parseChangelog(
			'## 1.0.0 — 2026-01-01\n\npara one\n\npara two\n',
		);
		expect(section.body).toBe('para one\n\npara two');
	});
	it('accepts hyphen, en dash and a missing date', () => {
		const parsed = parseChangelog(
			'## 1.0.1 - 2026-01-02\n- a\n## 1.0.2 – 2026-01-03\n- b\n## 1.0.3\n- c\n',
		);
		expect(parsed.map((s) => [s.version, s.date])).toEqual([
			['1.0.1', '2026-01-02'],
			['1.0.2', '2026-01-03'],
			['1.0.3', ''],
		]);
	});
	it('skips headings that are not versions', () => {
		const parsed = parseChangelog(
			'## Unreleased\n- x\n## 1.0.0 — 2026-01-01\n- y\n',
		);
		expect(versions(parsed)).toEqual(['1.0.0']);
		expect(parsed[0].body).toBe('- y');
	});
	it('returns nothing for empty or section-less input', () => {
		expect(parseChangelog('')).toEqual([]);
		expect(parseChangelog('just text\nno headings')).toEqual([]);
		expect(parseChangelog(undefined as unknown as string)).toEqual([]);
	});
});

describe('whatsNewSections', () => {
	it('shows nothing on a fresh install', () => {
		expect(
			whatsNewSections(sections, {
				lastSeen: null,
				current: '0.9.8',
				hasPriorData: false,
			}),
		).toEqual([]);
		expect(
			whatsNewSections(sections, {
				lastSeen: '',
				current: '0.9.8',
				hasPriorData: false,
			}),
		).toEqual([]);
	});
	it('shows only the current section for an existing user without the key', () => {
		expect(
			versions(
				whatsNewSections(sections, {
					lastSeen: null,
					current: '0.9.8',
					hasPriorData: true,
				}),
			),
		).toEqual(['0.9.8']);
	});
	it('shows nothing for prior data when the current version has no section', () => {
		expect(
			whatsNewSections(sections, {
				lastSeen: null,
				current: '0.9.9',
				hasPriorData: true,
			}),
		).toEqual([]);
	});
	it('shows every skipped version newest first, capped at current', () => {
		expect(
			versions(
				whatsNewSections(sections, {
					lastSeen: '0.9.6',
					current: '0.9.8',
					hasPriorData: true,
				}),
			),
		).toEqual(['0.9.8', '0.9.7']);
		expect(
			versions(
				whatsNewSections(sections, {
					lastSeen: '0.9.6',
					current: '0.9.10',
					hasPriorData: true,
				}),
			),
		).toEqual(['0.9.10', '0.9.8', '0.9.7']);
	});
	it('shows nothing for the same version or a downgrade', () => {
		expect(
			whatsNewSections(sections, {
				lastSeen: '0.9.8',
				current: '0.9.8',
				hasPriorData: true,
			}),
		).toEqual([]);
		expect(
			whatsNewSections(sections, {
				lastSeen: '0.9.10',
				current: '0.9.8',
				hasPriorData: true,
			}),
		).toEqual([]);
	});
});

describe('recentSections', () => {
	it('returns sections up to current, newest first', () => {
		expect(versions(recentSections(sections, '0.9.8'))).toEqual([
			'0.9.8',
			'0.9.7',
		]);
	});
	it('honours the limit', () => {
		expect(versions(recentSections(sections, '0.9.10', 2))).toEqual([
			'0.9.10',
			'0.9.8',
		]);
	});
});

describe('real changelog', () => {
	it('parses 0.9.9 without newlines inside bullets', () => {
		const markdown = readFileSync(
			join(__dirname, '../../../src-tauri/CHANGELOG.md'),
			'utf8',
		);
		const section = parseChangelog(markdown).find((s) => s.version === '0.9.9');
		expect(section).toBeDefined();
		expect(section!.date).toBe('2026-09-30');
		for (const line of section!.body.split('\n')) {
			expect(line.startsWith('- ')).toBe(true);
		}
	});
});
