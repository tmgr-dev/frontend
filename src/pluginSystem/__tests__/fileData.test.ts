import { encodeFile } from '../fileData';

it('gives text for text files and base64 for everything', () => {
	const text = new TextEncoder().encode('# Привет');
	expect(encodeFile(text, 'text/markdown', 'a.md')).toEqual({
		size: text.length,
		base64: Buffer.from(text).toString('base64'),
		text: '# Привет',
	});
	const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0xff]);
	expect(encodeFile(png, 'image/png', 'a.png')).toEqual({
		size: 5,
		base64: 'iVBOR/8=',
		text: null,
	});
	expect(
		encodeFile(new Uint8Array([0xff, 0xfe]), 'text/plain', 'bad.txt').text,
	).toBeNull();
	expect(() =>
		encodeFile(new Uint8Array(5 * 1024 * 1024 + 1), null, 'big'),
	).toThrow('5 MB');
});
