import {
	MAX_IMAGE_DATA_BYTES,
	MAX_IMAGE_TREE_BYTES,
	NO_LINKS,
	sanitizeTree,
	type ImageContext,
} from '../uiTree';

const images: ImageContext = { allowedDomains: ['img.example.com'] };

const dataUrl = (mime: string, bytes: Uint8Array) =>
	`data:${mime};base64,${Buffer.from(bytes).toString('base64')}`;

const pad = (head: number[], length = 64) => {
	const out = new Uint8Array(length);
	out.set(head);
	return out;
};

const PNG = pad([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const JPEG = pad([0xff, 0xd8, 0xff, 0xe0]);
const GIF = pad([...Buffer.from('GIF89a')]);
const GIF87 = pad([...Buffer.from('GIF87a')]);
const WEBP = pad([...Buffer.from('RIFF'), 0, 0, 0, 0, ...Buffer.from('WEBP')]);

const run = (node: unknown, minor = 13, ctx: ImageContext = images) =>
	sanitizeTree(node, NO_LINKS, minor, ctx);

const image = (src: string, extra: Record<string, unknown> = {}) => ({
	type: 'image',
	src,
	alt: 'A picture',
	...extra,
});

describe('image data sources', () => {
	it.each([
		['image/png', PNG],
		['image/jpeg', JPEG],
		['image/gif', GIF],
		['image/gif', GIF87],
		['image/webp', WEBP],
	])('keeps a %s', (mime, bytes) => {
		const src = dataUrl(mime, bytes);
		expect(run(image(src))).toEqual({ type: 'image', src, alt: 'A picture' });
	});

	it('drops svg, other types and parameters', () => {
		for (const src of [
			dataUrl('image/svg+xml', PNG),
			dataUrl('image/bmp', PNG),
			dataUrl('text/html', PNG),
			`data:image/png;charset=utf-8;base64,${Buffer.from(PNG).toString('base64')}`,
			`data:image/png,${Buffer.from(PNG).toString('base64')}`,
			`DATA:image/png;base64,${Buffer.from(PNG).toString('base64')}`,
		]) {
			expect(run(image(src))).toBeNull();
		}
	});

	it('drops bad base64', () => {
		const good = Buffer.from(PNG).toString('base64');
		for (const body of [
			good.slice(0, -1),
			`${good} `,
			` ${good}`,
			`${good.slice(0, 8)}\n${good.slice(8)}`,
			`${good.slice(0, 8)}*${good.slice(9)}`,
			`${good}=`,
			'=AAA',
			'',
		]) {
			expect(run(image(`data:image/png;base64,${body}`))).toBeNull();
		}
		expect(
			run(image(`data:image/png;base64,${good.replace(/\+|\//g, '-')}_`)),
		).toBeNull();
	});

	it('drops bytes that are not the declared type', () => {
		expect(run(image(dataUrl('image/png', JPEG)))).toBeNull();
		expect(run(image(dataUrl('image/jpeg', PNG)))).toBeNull();
		expect(run(image(dataUrl('image/gif', WEBP)))).toBeNull();
		expect(
			run(
				image(
					dataUrl(
						'image/webp',
						pad([...Buffer.from('RIFF'), 0, 0, 0, 0, ...Buffer.from('WAVE')]),
					),
				),
			),
		).toBeNull();
		expect(run(image(dataUrl('image/png', pad([0x89, 0x50]))))).toBeNull();
	});

	it('caps one image at 512 KB decoded', () => {
		const limit = pad([0x89, 0x50, 0x4e, 0x47], MAX_IMAGE_DATA_BYTES);
		const over = pad([0x89, 0x50, 0x4e, 0x47], MAX_IMAGE_DATA_BYTES + 1);
		expect(run(image(dataUrl('image/png', limit)))).not.toBeNull();
		expect(run(image(dataUrl('image/png', over)))).toBeNull();
	});

	it('caps all data images of a tree at 2 MB, dropping the later ones', () => {
		const big = dataUrl(
			'image/png',
			pad([0x89, 0x50, 0x4e, 0x47], MAX_IMAGE_DATA_BYTES),
		);
		const count = MAX_IMAGE_TREE_BYTES / MAX_IMAGE_DATA_BYTES;
		const tree: any = run({
			type: 'stack',
			children: Array.from({ length: count + 2 }, () => image(big)),
		});
		expect(tree.children).toHaveLength(count);
		const small = dataUrl('image/png', PNG);
		const after: any = run({
			type: 'stack',
			children: [
				...Array.from({ length: count }, () => image(big)),
				image(small),
			],
		});
		expect(after.children).toHaveLength(count);
	});
});

describe('image https sources', () => {
	const ok = 'https://img.example.com/a/b.png?x=1';

	it('keeps a declared https host', () => {
		expect(run(image(ok))).toEqual({
			type: 'image',
			src: ok,
			alt: 'A picture',
		});
		expect(run(image('https://img.example.com:443/a.png'))).not.toBeNull();
	});

	it('drops undeclared hosts and look-alikes', () => {
		for (const src of [
			'https://other.example.com/a.png',
			'https://example.com/a.png',
			'https://img.example.com.evil.com/a.png',
			'https://evil.com/img.example.com',
			'https://img.example.com@evil.com/a.png',
			'https://img.example.com./a.png',
		]) {
			expect(run(image(src))).toBeNull();
		}
		expect(run(image(ok), 13, { allowedDomains: [] })).toBeNull();
		expect(run(image(ok), 13, undefined)).not.toBeNull();
		expect(sanitizeTree(image(ok), NO_LINKS, 13)).toBeNull();
	});

	it('drops every other scheme and shape', () => {
		for (const src of [
			'http://img.example.com/a.png',
			'blob:https://img.example.com/uuid',
			'tmgrfile://img.example.com/a.png',
			'file:///etc/passwd',
			'ipc://localhost/x',
			'tauri://localhost/a.png',
			'asset://localhost/a.png',
			'javascript:alert(1)',
			'/a.png',
			'a.png',
			'//img.example.com/a.png',
			'data:image/svg+xml;base64,PHN2Zy8+',
			'',
		]) {
			expect(run(image(src))).toBeNull();
		}
	});

	it('drops userinfo, ports, whitespace and long URLs', () => {
		for (const src of [
			'https://user:pw@img.example.com/a.png',
			'https://user@img.example.com/a.png',
			'https://img.example.com:8443/a.png',
			' https://img.example.com/a.png',
			'https://img.example.com/a b.png',
			'https://img.example.com/\n.png',
			`https://img.example.com/${'a'.repeat(2000)}`,
		]) {
			expect(run(image(src))).toBeNull();
		}
	});

	it('drops IP literals and localhost even if a context lists them', () => {
		const ctx = {
			allowedDomains: ['127.0.0.1', 'localhost', '[::1]', '::1', 'intranet'],
		};
		for (const src of [
			'https://127.0.0.1/a.png',
			'https://localhost/a.png',
			'https://[::1]/a.png',
			'https://intranet/a.png',
		]) {
			expect(run(image(src), 13, ctx)).toBeNull();
		}
	});
});

describe('image fields and gating', () => {
	const src = dataUrl('image/png', PNG);

	it('requires alt', () => {
		expect(run({ type: 'image', src })).toBeNull();
		expect(run(image(src, { alt: '' }))).toBeNull();
		expect(run(image(src, { alt: '   ' }))).toBeNull();
		expect(run(image(src, { alt: 5 }))).toBeNull();
	});

	it('cuts alt to 200 characters', () => {
		expect((run(image(src, { alt: 'a'.repeat(300) })) as any).alt).toHaveLength(
			200,
		);
	});

	it('clamps width to 16..800 and drops what is not a number', () => {
		const width = (value: unknown) =>
			(run(image(src, { width: value })) as any).width;
		expect(width(64)).toBe(64);
		expect(width(1)).toBe(16);
		expect(width(-5)).toBe(16);
		expect(width(5000)).toBe(800);
		expect(width(100.6)).toBe(101);
		expect(width('100')).toBeUndefined();
		expect(width(NaN)).toBeUndefined();
		expect(width(Infinity)).toBeUndefined();
		expect('width' in (run(image(src)) as any)).toBe(false);
	});

	it('exists from engines 1.13 only', () => {
		expect(run(image(src), 12)).toBeNull();
		expect(run(image(src), 10)).toBeNull();
		expect(run(image(src), 13)).not.toBeNull();
	});

	it('does not accept an image as a bare string or with a non-string src', () => {
		expect(run(image(5 as any))).toBeNull();
		expect(run({ type: 'image', alt: 'x' })).toBeNull();
	});

	it('counts toward the node budget and the depth limit', () => {
		const tree: any = run({
			type: 'stack',
			children: Array.from({ length: 500 }, () => ({
				type: 'stack',
				children: Array.from({ length: 10 }, () => image(src)),
			})),
		});
		const total = tree.children.reduce(
			(sum: number, row: any) => sum + 1 + row.children.length,
			1,
		);
		expect(total).toBeLessThanOrEqual(3000);
		expect(
			tree.children.reduce(
				(sum: number, row: any) => sum + row.children.length,
				0,
			),
		).toBeLessThan(5000);
		let deep: any = image(src);
		for (let i = 0; i < 9; i++) deep = { type: 'stack', children: [deep] };
		expect(JSON.stringify(run(deep))).not.toContain('"image"');
	});
});
