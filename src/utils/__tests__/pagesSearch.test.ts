import {
	availableSearchTabs,
	canSearch,
	cleanSnippet,
	pageHitUrl,
	pageTypeLabel,
	resolveSearchTab,
} from '../pagesSearch';

describe('search tabs', () => {
	it('offers Tasks and Pages when pages are available', () => {
		expect(availableSearchTabs(true).map((t) => t.key)).toEqual([
			'tasks',
			'pages',
		]);
	});
	it('hides Pages when unavailable and falls back to tasks', () => {
		expect(availableSearchTabs(false).map((t) => t.key)).toEqual(['tasks']);
		expect(resolveSearchTab('pages', false)).toBe('tasks');
		expect(resolveSearchTab('pages', true)).toBe('pages');
	});
});

describe('canSearch', () => {
	it('needs at least two non-blank characters', () => {
		expect(canSearch('')).toBe(false);
		expect(canSearch(' a ')).toBe(false);
		expect(canSearch('ab')).toBe(true);
	});
});

describe('pageHitUrl', () => {
	it('opens the page by slug inside the workspace', () => {
		expect(pageHitUrl('tmgr', { slug: 'plan-q4' })).toBe('/tmgr/pages/plan-q4');
	});
});

describe('pageTypeLabel', () => {
	it('labels known types and passes unknown ones through', () => {
		expect(pageTypeLabel('context')).toBe('Context');
		expect(pageTypeLabel('x')).toBe('x');
	});
});

describe('cleanSnippet', () => {
	it('keeps link text and drops the tmgr href', () => {
		expect(cleanSnippet('see [TMGR-1](tmgr://task/305) now')).toBe(
			'see TMGR-1 now',
		);
	});
	it('drops images, section markers and heading hashes', () => {
		expect(
			cleanSnippet(
				'## Notes <!-- tmgr:section id="a" owner="agents" --> ![x](tmgr://file/1) end',
			),
		).toBe('Notes end');
	});
});
