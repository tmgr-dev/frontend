import type { Page } from '@/actions/tmgr/pages';
import { exportSinglePage } from '../markdownIo/exportBundle';
import {
	loadPageExporter,
	pageShareLinks,
	pageShareMarkdown,
} from '../shareLinks';

const web = { id: 7, code: 'demo' };
const local = { id: -1, code: 'local' };

const page = {
	id: 3,
	slug: 'roadmap',
	title: 'Road: map',
	type: 'note',
	parent_id: null,
	position: 0,
	pinned: false,
	updated_at: '2026-10-21T00:00:00Z',
	workspace_id: 7,
	body: '# Roadmap\n\nUnsaved line\n',
	properties: { status: 'draft' },
	version: 2,
} as unknown as Page;

describe('pageShareLinks', () => {
	it('builds web and app links in a cloud workspace', () => {
		expect(pageShareLinks(page, web)).toEqual({
			web: 'https://tmgr.dev/demo/pages/roadmap',
			app: 'tmgr://page/demo/roadmap',
		});
	});

	it('has no web link in a local workspace', () => {
		expect(pageShareLinks(page, local)).toEqual({
			web: null,
			app: 'tmgr://page/local/roadmap',
		});
	});

	it('encodes the slug in the web link and drops the app link it cannot open', () => {
		expect(pageShareLinks({ slug: 'план q4' }, web)).toEqual({
			web: 'https://tmgr.dev/demo/pages/%D0%BF%D0%BB%D0%B0%D0%BD%20q4',
			app: null,
		});
	});

	it('drops the app link for an over-long slug or workspace code', () => {
		expect(pageShareLinks({ slug: 'a'.repeat(121) }, web).app).toBeNull();
		expect(pageShareLinks({ slug: 'a'.repeat(120) }, web).app).not.toBeNull();
		expect(
			pageShareLinks(page, { id: 1, code: 'w'.repeat(65) }).app,
		).toBeNull();
	});
});

describe('pageShareMarkdown', () => {
	const expected = exportSinglePage(page, 'demo').text;

	it('loads the exporter on first use and equals Export as Markdown', async () => {
		const cold = pageShareMarkdown(page, 'demo');
		expect(cold).toBeInstanceOf(Promise);
		await expect(cold).resolves.toBe(expected);
	});

	it('returns the text synchronously once the exporter is loaded', async () => {
		await loadPageExporter();
		expect(pageShareMarkdown(page, 'demo')).toBe(expected);
		expect(expected).toContain('Unsaved line');
	});

	it('still copies Markdown in a local workspace', async () => {
		const text = await pageShareMarkdown(page, local.code);
		expect(text).toBe(exportSinglePage(page, local.code).text);
		expect(text).toContain('# Roadmap');
	});
});
