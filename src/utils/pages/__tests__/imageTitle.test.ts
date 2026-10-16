import {
	formatImageTitle,
	isFilenameAlt,
	parseImageTitle,
	pngPixelRatio,
} from '../imageTitle';

const empty = {
	layout: null,
	width: null,
	height: null,
	source: null,
	title: '',
};

describe('parseImageTitle', () => {
	it.each([
		['', empty],
		[undefined, empty],
		[' =300x200', { ...empty, width: 300, height: 200 }],
		['=300x200', { ...empty, width: 300, height: 200 }],
		[' =640x', { ...empty, width: 640 }],
		[' =x120', { ...empty, height: 120 }],
		[
			'right-50 =300x200',
			{ ...empty, layout: 'right-50', width: 300, height: 200 },
		],
		['left-50', { ...empty, layout: 'left-50' }],
		['full-width', { ...empty, layout: 'full-width' }],
		['My title =640x', { ...empty, title: 'My title', width: 640 }],
		['right-50 My title', { ...empty, layout: 'right-50', title: 'My title' }],
		[
			'source=diagrams =100x100',
			{ ...empty, source: 'diagrams', width: 100, height: 100 },
		],
		['just a title', { ...empty, title: 'just a title' }],
		['size=300x200', { ...empty, title: 'size=300x200' }],
	])('parses %j', (raw, expected) => {
		expect(parseImageTitle(raw)).toEqual(expected);
	});
});

describe('formatImageTitle', () => {
	it.each([
		[empty, ''],
		[{ ...empty, width: 300, height: 200 }, ' =300x200'],
		[{ ...empty, width: 640 }, ' =640x'],
		[
			{ ...empty, layout: 'right-50', width: 300, height: 200 },
			'right-50 =300x200',
		],
		[{ ...empty, layout: 'full-width' }, 'full-width'],
		[{ ...empty, title: 'My title', width: 640 }, 'My title =640x'],
		[{ ...empty, layout: 'left-50', title: 'Kept' }, 'left-50 Kept'],
		[
			{ ...empty, source: 'diagrams', width: 1, height: 2 },
			'source=diagrams =1x2',
		],
	] as const)('formats %j', (parts, expected) => {
		expect(formatImageTitle(parts)).toBe(expected);
	});

	it.each([
		' =300x200',
		'right-50 =300x200',
		'left-50 Kept =10x20',
		'full-width',
		'My title =640x',
		'source=diagrams =100x100',
		'',
	])('round-trips %j', (raw) => {
		expect(formatImageTitle(parseImageTitle(raw))).toBe(raw);
	});

	it('rounds sizes to whole pixels', () => {
		expect(formatImageTitle({ ...empty, width: 300.6, height: 199.2 })).toBe(
			' =301x199',
		);
	});
});

describe('isFilenameAlt', () => {
	it.each([
		['Screenshot 2026-10-15 at 12.31.22.png', true],
		['photo.JPEG', true],
		['diagram.webp', true],
		['', false],
		['A sunny day', false],
		['see report.pdf', false],
		['two\nlines.png', false],
	])('%j → %s', (alt, expected) => {
		expect(isFilenameAlt(alt)).toBe(expected);
	});
});

const pngWithPhys = (ppm: number | null): Uint8Array => {
	const sig = [137, 80, 78, 71, 13, 10, 26, 10];
	const chunk = (type: string, data: number[]) => {
		const len = data.length;
		return [
			(len >>> 24) & 255,
			(len >>> 16) & 255,
			(len >>> 8) & 255,
			len & 255,
			...Array.from(type, (c) => c.charCodeAt(0)),
			...data,
			0,
			0,
			0,
			0,
		];
	};
	const u32 = (n: number) => [
		(n >>> 24) & 255,
		(n >>> 16) & 255,
		(n >>> 8) & 255,
		n & 255,
	];
	const ihdr = chunk('IHDR', [...u32(10), ...u32(10), 8, 2, 0, 0, 0]);
	const phys = ppm === null ? [] : chunk('pHYs', [...u32(ppm), ...u32(ppm), 1]);
	return new Uint8Array([...sig, ...ihdr, ...phys, ...chunk('IEND', [])]);
};

describe('pngPixelRatio', () => {
	it('is 2 for a 144 dpi screenshot', () => {
		expect(pngPixelRatio(pngWithPhys(5669))).toBe(2);
	});

	it('is 1 for 72 dpi', () => {
		expect(pngPixelRatio(pngWithPhys(2835))).toBe(1);
	});

	it('is 1 without pHYs', () => {
		expect(pngPixelRatio(pngWithPhys(null))).toBe(1);
	});

	it('is 1 for non-PNG bytes', () => {
		expect(pngPixelRatio(new Uint8Array([255, 216, 255, 224]))).toBe(1);
	});

	it('is 1 when pHYs unit is unknown', () => {
		const bytes = pngWithPhys(5669);
		bytes[bytes.length - 12 - 4 - 1] = 0;
		expect(pngPixelRatio(bytes)).toBe(1);
	});
});
