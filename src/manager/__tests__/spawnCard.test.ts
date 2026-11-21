import {
	formatLeft,
	parseSpawnRequest,
	spawnStatusText,
	spawnView,
} from '../spawnCard';
import type { SpawnCardWire } from '../types';

const request = (extra: Record<string, unknown> = {}) =>
	JSON.stringify({
		v: 1,
		profile: 'claude',
		cwd: '/work/app',
		mode: 'default',
		target_id: 'mx_a',
		...extra,
	});

const wire = (req: string): SpawnCardWire => ({
	summary: 'Start claude in /work/app',
	request: req,
	nonce: 'n',
	exp_ms: 1000,
	target_id: 'mx_a',
	target_fp: 'fp',
});

describe('spawn card view', () => {
	it('parses a proposal and lists its lines', () => {
		const view = spawnView(wire(request({ task: 'TM-1', name: 'fix' })));
		expect(view.profile).toBe('claude');
		expect(view.bypass).toBe(false);
		expect(view.lines.map((l) => l.key)).toEqual([
			'cwd',
			'mode',
			'task',
			'name',
			'target',
		]);
	});

	it('warns on bypass mode', () => {
		const view = spawnView(wire(request({ mode: 'bypass' })));
		expect(view.bypass).toBe(true);
		expect(view.lines.find((l) => l.key === 'mode')?.warn).toBe(true);
	});

	it('falls back to the summary on bad JSON or missing fields', () => {
		expect(parseSpawnRequest('{nope')).toBeNull();
		expect(parseSpawnRequest('{"profile":"claude"}')).toBeNull();
		const view = spawnView(wire('{nope'));
		expect(view).toEqual({
			summary: 'Start claude in /work/app',
			profile: null,
			lines: [],
			bypass: false,
		});
	});
});

describe('spawn card status', () => {
	it('formats the countdown', () => {
		expect(formatLeft(65_000)).toBe('1:05');
		expect(formatLeft(-5)).toBe('0:00');
	});

	it('counts down while issued and expires', () => {
		const live = spawnStatusText(
			{ state: 'issued', exp_ms: 100_000 },
			0,
			40_000,
		);
		expect(live).toMatchObject({
			key: 'manager.spawn.waiting',
			params: { left: '1:00' },
			final: false,
		});
		expect(
			spawnStatusText({ state: 'issued', exp_ms: 100_000 }, 0, 100_000).key,
		).toBe('manager.spawn.expired');
	});

	it('uses the wire expiry when the status has none', () => {
		expect(spawnStatusText({ state: 'issued' }, 60_000, 0).params.left).toBe(
			'1:00',
		);
	});

	it('maps the remaining states', () => {
		expect(spawnStatusText({ state: 'approved' }, 0, 0).key).toBe(
			'manager.spawn.approved',
		);
		expect(spawnStatusText({ state: 'claimed' }, 0, 0).key).toBe(
			'manager.spawn.claimed',
		);
		expect(
			spawnStatusText({ state: 'spawned', session: 's_1' }, 0, 0),
		).toMatchObject({ params: { session: 's_1' }, final: true, tone: 'ok' });
		expect(
			spawnStatusText({ state: 'refused', reason: 'expired' }, 0, 0).key,
		).toBe('manager.spawn.expired');
		expect(
			spawnStatusText({ state: 'refused', reason: 'untrusted' }, 0, 0),
		).toMatchObject({
			key: 'manager.spawn.refused',
			params: { reason: 'untrusted' },
			final: true,
		});
	});
});
