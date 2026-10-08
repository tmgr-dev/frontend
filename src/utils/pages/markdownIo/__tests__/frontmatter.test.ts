import { parseFrontmatter, serializeFrontmatter } from '../frontmatter';

describe('frontmatter', () => {
	it('round-trips a plain page', () => {
		const out = serializeFrontmatter({
			title: 'Hello: world #1',
			type: 'plain',
			properties: { ignored: true },
			tmgr: { workspace: 'acme', id: 7 },
		});
		const parsed = parseFrontmatter(`${out}Body text\n`);
		expect(parsed.frontmatter).toEqual({
			title: 'Hello: world #1',
			type: 'plain',
			tmgr: { workspace: 'acme', id: 7 },
		});
		expect(parsed.body).toBe('Body text\n');
		expect(parsed.ignoredKeys).toEqual([]);
	});

	it('round-trips person aliases', () => {
		const properties = {
			user_id: null,
			aliases: [{ source: 'github', native_id: 'octo', display: 'Octo' }],
			network: 'personal',
			company: 'Example Co',
			role: null,
			last_contact_at: null,
		};
		const out = serializeFrontmatter({
			title: 'Ann',
			type: 'person',
			properties,
			tmgr: { workspace: 'acme', id: 1 },
		});
		expect(parseFrontmatter(out).frontmatter?.properties).toEqual(properties);
	});

	it('round-trips meeting properties and keeps dates as strings', () => {
		const properties = {
			date: '2026-10-08',
			participants: ['tmgr://page/3'],
			related_tasks: [4, 5],
		};
		const out = serializeFrontmatter({
			title: 'Sync',
			type: 'meeting',
			properties,
			tmgr: { workspace: 'acme', id: 2 },
		});
		expect(parseFrontmatter(out).frontmatter?.properties).toEqual(properties);
	});

	it('lists ignored keys and strips the block', () => {
		const parsed = parseFrontmatter(
			'---\ntitle: T\ntags: [a, b]\naliases:\n  - x\ncreated: 2026-01-01\n---\n\n# Heading\n',
		);
		expect(parsed.frontmatter?.title).toBe('T');
		expect(parsed.ignoredKeys).toEqual(['tags', 'aliases', 'created']);
		expect(parsed.body).toBe('# Heading\n');
	});

	it('treats malformed YAML as no frontmatter with a warning', () => {
		const text = '---\ntitle: [unclosed\n---\nBody\n';
		const parsed = parseFrontmatter(text);
		expect(parsed.frontmatter).toBeNull();
		expect(parsed.body).toBe(text);
		expect(parsed.warning).toMatch(/not valid YAML/);
	});

	it('does not treat a lone horizontal rule as frontmatter', () => {
		const parsed = parseFrontmatter('---\nno closing fence');
		expect(parsed.frontmatter).toBeNull();
		expect(parsed.warning).toBeNull();
	});

	it('handles CRLF', () => {
		const parsed = parseFrontmatter('---\r\ntitle: A\r\n---\r\nB');
		expect(parsed.frontmatter?.title).toBe('A');
		expect(parsed.body).toBe('B');
	});

	it('never evaluates custom tags', () => {
		const parsed = parseFrontmatter(
			'---\ntitle: A\ncmd: !!js/function x\n---\nB',
		);
		expect(parsed.frontmatter?.properties).toBeUndefined();
		expect(
			parsed.frontmatter === null || parsed.frontmatter.title === 'A',
		).toBe(true);
	});
});
