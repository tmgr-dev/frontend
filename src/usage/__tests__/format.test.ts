import {
	ago,
	barTone,
	count,
	limitLabel,
	money,
	percent,
	span,
	toolName,
} from '../format';

describe('usage format', () => {
	it('formats money', () => {
		expect(money(0)).toBe('$0.00');
		expect(money(0.004)).toBe('<$0.01');
		expect(money(12.345)).toBe('$12.35');
		expect(money(1234.5)).toBe('$1,235');
		expect(money(null)).toBe('—');
	});

	it('formats counts with K/M/B', () => {
		expect(count(0)).toBe('0');
		expect(count(999)).toBe('999');
		expect(count(1500)).toBe('1.5K');
		expect(count(2_000_000)).toBe('2M');
		expect(count(3_400_000_000)).toBe('3.4B');
		expect(count(123_456)).toBe('123K');
		expect(count(undefined)).toBe('—');
	});

	it('formats reset countdowns', () => {
		expect(span(null)).toBe('—');
		expect(span(0)).toBe('now');
		expect(span(30)).toBe('<1m');
		expect(span(125 * 60)).toBe('2h 5m');
		expect(span(90_000)).toBe('1d 1h');
		expect(ago(10)).toBe('just now');
		expect(ago(600)).toBe('10m ago');
	});

	it('labels limits and tools', () => {
		expect(toolName('claude')).toBe('Claude Code');
		expect(toolName('gemini')).toBe('Gemini');
		expect(limitLabel({ tool: 'codex', window_label: 'Weekly' })).toBe(
			'Codex · Weekly',
		);
	});

	it('clamps percent and picks a tone', () => {
		expect(percent(-5)).toBe(0);
		expect(percent(133)).toBe(100);
		expect(percent(42.6)).toBe(43);
		expect(percent(NaN)).toBe(0);
		expect(barTone(10)).toBe('ok');
		expect(barTone(70)).toBe('warn');
		expect(barTone(95)).toBe('danger');
	});
});
