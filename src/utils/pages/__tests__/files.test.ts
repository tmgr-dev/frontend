import { fileMarkdown, formatFileSize, imageFilesOf } from '../files';

const file = (name: string, type: string) => ({ name, type } as File);

describe('page file helpers', () => {
	it('formats sizes', () => {
		expect(formatFileSize(null)).toBe('');
		expect(formatFileSize(512)).toBe('512 Б');
		expect(formatFileSize(2048)).toBe('2 КБ');
		expect(formatFileSize(3 * 1024 * 1024)).toBe('3.0 МБ');
	});

	it('keeps only images', () => {
		const list = [file('a.png', 'image/png'), file('b.pdf', 'application/pdf')];
		expect(imageFilesOf(list).map((f) => f.name)).toEqual(['a.png']);
		expect(imageFilesOf(null)).toEqual([]);
	});

	it('builds the image markdown with an escaped alt', () => {
		expect(fileMarkdown(7)).toBe('![](tmgr://file/7)');
		expect(fileMarkdown(7, 'a[1]')).toBe('![a\\[1\\]](tmgr://file/7)');
	});
});
