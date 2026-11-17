import { createDevLogMirror } from '../devLogMirror';

const line = (message: string, level: 'info' | 'warn' | 'error' = 'info') => ({
	at: 0,
	level,
	message,
});

const make = (enabled = true) => {
	let now = 0;
	const written: string[] = [];
	const mirror = createDevLogMirror({
		enabled: () => enabled,
		write: (_level, text) => void written.push(text),
		now: () => now,
	});
	return { mirror, written, tick: (ms: number) => (now += ms) };
};

describe('createDevLogMirror', () => {
	it('writes a tagged single line', () => {
		const { mirror, written } = make();
		mirror('acme.x', line('boom', 'error'));
		expect(written).toEqual(['[plugin] acme.x [error] boom']);
	});

	it('cannot forge another log line with line breaks', () => {
		const { mirror, written } = make();
		mirror('acme.x', line('a\n[plugin] other.id running\r\tb'));
		expect(written).toHaveLength(1);
		expect(written[0]).not.toMatch(/[\n\r]/);
	});

	it('cuts long messages', () => {
		const { mirror, written } = make();
		mirror('acme.x', line('x'.repeat(5000)));
		expect(written[0].length).toBeLessThan(600);
	});

	it('writes nothing when disabled', () => {
		const { mirror, written } = make(false);
		mirror('acme.x', line('hi'));
		expect(written).toEqual([]);
	});

	it('drops a flood and reports the count once the plugin calms down', () => {
		const { mirror, written, tick } = make();
		for (let i = 0; i < 80; i++) mirror('acme.x', line(`m${i}`));
		expect(written).toHaveLength(50);
		tick(1000);
		mirror('acme.x', line('after'));
		expect(written.slice(-2)).toEqual([
			'[plugin] acme.x [warn] 30 log lines dropped',
			'[plugin] acme.x [info] after',
		]);
	});
});
