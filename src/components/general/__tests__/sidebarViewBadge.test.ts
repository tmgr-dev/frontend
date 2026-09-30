import {
	BADGE_DOT_CLASS,
	BADGE_TONE_CLASS,
	viewBadgeAriaLabel,
	viewBadgeLabel,
} from '../sidebarViewBadge';

const count = (value: number) => ({
	count: value,
	text: null,
	tone: 'default' as const,
});
const text = (value: string) => ({
	count: null,
	text: value,
	tone: 'default' as const,
});

describe('viewBadgeLabel', () => {
	it('shows the number as is up to 99', () => {
		expect(viewBadgeLabel(count(1))).toBe('1');
		expect(viewBadgeLabel(count(99))).toBe('99');
	});

	it('caps larger numbers at 99+', () => {
		expect(viewBadgeLabel(count(100))).toBe('99+');
		expect(viewBadgeLabel(count(12000))).toBe('99+');
	});

	it('shows text as is', () => {
		expect(viewBadgeLabel(text('new'))).toBe('new');
	});
});

describe('viewBadgeAriaLabel', () => {
	it('reports the exact count, even above 99', () => {
		expect(viewBadgeAriaLabel('Telegram', count(12))).toBe(
			'Telegram, 12 pending',
		);
		expect(viewBadgeAriaLabel('Telegram', count(150))).toBe(
			'Telegram, 150 pending',
		);
	});

	it('appends text badges', () => {
		expect(viewBadgeAriaLabel('Telegram', text('new'))).toBe('Telegram, new');
	});

	it('is undefined without a badge', () => {
		expect(viewBadgeAriaLabel('Telegram', undefined)).toBeUndefined();
		expect(viewBadgeAriaLabel('Telegram', null)).toBeUndefined();
	});
});

describe('tone classes', () => {
	const tones = ['default', 'info', 'warning', 'danger'] as const;

	it('covers every tone', () => {
		expect(Object.keys(BADGE_TONE_CLASS).sort()).toEqual([...tones].sort());
		expect(Object.keys(BADGE_DOT_CLASS).sort()).toEqual([...tones].sort());
	});

	it('has a dark variant on every coloured tone', () => {
		for (const tone of ['info', 'warning', 'danger'] as const) {
			expect(BADGE_TONE_CLASS[tone]).toContain('dark:');
			expect(BADGE_DOT_CLASS[tone]).toContain('dark:');
		}
	});

	it('maps tones to blue, amber and red', () => {
		expect(BADGE_TONE_CLASS.info).toContain('blue');
		expect(BADGE_TONE_CLASS.warning).toContain('amber');
		expect(BADGE_TONE_CLASS.danger).toContain('red');
	});
});
