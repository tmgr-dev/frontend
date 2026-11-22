import {
	clampWidth,
	DEFAULT_WIDTH,
	loadWidth,
	MAX_WIDTH,
	MIN_CHAT_WIDTH,
	MIN_WIDTH,
	saveWidth,
	shellColumns,
	type WidthStorage,
} from '../panelWidths';

const memory = (): WidthStorage & { data: Record<string, string> } => {
	const data: Record<string, string> = {};
	return {
		data,
		getItem: (k) => data[k] ?? null,
		setItem: (k, v) => {
			data[k] = v;
		},
	};
};

const broken: WidthStorage = {
	getItem: () => {
		throw new Error('blocked');
	},
	setItem: () => {
		throw new Error('blocked');
	},
};

describe('clampWidth', () => {
	it('keeps a width inside the side limits', () => {
		expect(clampWidth('left', 100, 1600, 340)).toBe(MIN_WIDTH.left);
		expect(clampWidth('left', 900, 1600, 340)).toBe(MAX_WIDTH.left);
		expect(clampWidth('right', 400, 1600, 300)).toBe(400);
	});

	it('leaves at least the minimum chat width', () => {
		expect(clampWidth('left', 520, 1000, 340)).toBe(
			1000 - 340 - MIN_CHAT_WIDTH,
		);
		expect(clampWidth('right', 560, 1000, 300)).toBe(
			1000 - 300 - MIN_CHAT_WIDTH,
		);
	});

	it('never goes under the side minimum even in a tiny window', () => {
		expect(clampWidth('left', 300, 500, 340)).toBe(MIN_WIDTH.left);
	});

	it('falls back to the default for a non-finite width', () => {
		expect(clampWidth('right', NaN, 1600, 300)).toBe(DEFAULT_WIDTH.right);
	});
});

describe('stored widths', () => {
	it('round-trips a width per side', () => {
		const s = memory();
		saveWidth('left', 410, s);
		saveWidth('right', 280, s);
		expect(loadWidth('left', s)).toBe(410);
		expect(loadWidth('right', s)).toBe(280);
	});

	it('uses the default when nothing or garbage is stored', () => {
		const s = memory();
		expect(loadWidth('left', s)).toBe(DEFAULT_WIDTH.left);
		s.data['manager.panel.left'] = 'wide';
		expect(loadWidth('left', s)).toBe(DEFAULT_WIDTH.left);
	});

	it('clamps a stored width that is out of range', () => {
		const s = memory();
		s.data['manager.panel.right'] = '5000';
		expect(loadWidth('right', s)).toBe(MAX_WIDTH.right);
	});

	it('survives a storage that throws', () => {
		expect(loadWidth('left', broken)).toBe(DEFAULT_WIDTH.left);
		expect(() => saveWidth('left', 300, broken)).not.toThrow();
	});

	it('survives missing storage', () => {
		expect(loadWidth('left', null)).toBe(DEFAULT_WIDTH.left);
		expect(() => saveWidth('left', 300, null)).not.toThrow();
	});
});

describe('shellColumns', () => {
	it('has three columns above 1100px', () => {
		expect(shellColumns(300, 340, 1300)).toBe('300px minmax(0, 1fr) 340px');
		expect(shellColumns(300, 340, 1101)).toBe('300px minmax(0, 1fr) 340px');
	});

	it('drops the right column at 1100px and below', () => {
		expect(shellColumns(300, 340, 1100)).toBe('300px minmax(0, 1fr)');
		expect(shellColumns(300, 340, 900)).toBe('300px minmax(0, 1fr)');
	});
});
