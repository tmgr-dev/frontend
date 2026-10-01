import { extractToc } from '../toc';

describe('extractToc', () => {
	it('lists ## headings only', () => {
		expect(extractToc('# Title\n## A\ntext\n### Sub\n## B\n')).toEqual([
			{ text: 'A', occurrence: 0 },
			{ text: 'B', occurrence: 0 },
		]);
	});

	it('skips headings inside fenced code', () => {
		expect(extractToc('```\n## no\n```\n## yes\n')).toEqual([
			{ text: 'yes', occurrence: 0 },
		]);
	});

	it('numbers repeated headings', () => {
		expect(extractToc('## A\n## A\n## A')).toEqual([
			{ text: 'A', occurrence: 0 },
			{ text: 'A', occurrence: 1 },
			{ text: 'A', occurrence: 2 },
		]);
	});

	it('strips closing hashes and inline markup', () => {
		expect(extractToc('## **Bold** [link](tmgr://page/1) ##\n')).toEqual([
			{ text: 'Bold link', occurrence: 0 },
		]);
	});

	it('includes headings inside managed sections', () => {
		expect(
			extractToc(
				'<!-- tmgr:section id="x" owner="agents" -->\n## Заметки агентов\n<!-- /tmgr:section -->\n',
			),
		).toEqual([{ text: 'Заметки агентов', occurrence: 0 }]);
	});

	it('requires a space after the hashes', () => {
		expect(extractToc('##nope\n')).toEqual([]);
	});

	it('is empty for an empty body', () => {
		expect(extractToc('')).toEqual([]);
	});
});
