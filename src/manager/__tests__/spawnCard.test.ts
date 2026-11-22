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
			'target',
			'cwd',
			'mode',
			'task',
			'name',
			'effect',
			'undo',
		]);
		expect(view.title).toEqual({
			key: 'manager.spawn.title',
			params: { profile: 'claude' },
		});
		expect(view.proposer).toEqual({ key: 'manager' });
		expect(view.chips).toEqual([]);
	});

	it('warns on bypass mode', () => {
		const view = spawnView(wire(request({ mode: 'bypass' })));
		expect(view.bypass).toBe(true);
		expect(view.chips).toEqual([{ key: 'bypass', tone: 'warn' }]);
		expect(view.lines.find((l) => l.key === 'mode')?.warn).toBe(true);
	});

	it('falls back to the summary on bad JSON or missing fields', () => {
		expect(parseSpawnRequest('{nope')).toBeNull();
		expect(parseSpawnRequest('{"profile":"claude"}')).toBeNull();
		const view = spawnView(wire('{nope'));
		expect(view).toMatchObject({
			summary: 'Start claude in /work/app',
			profile: null,
			structured: false,
			title: null,
			lines: [],
			chips: [],
			bypass: false,
			payload: '{nope',
		});
	});
});

describe('spawn card kinds', () => {
	it('shows a stop proposal as irreversible', () => {
		const view = spawnView(
			wire(
				JSON.stringify({
					kind: 'stop',
					session: 'opencode',
					target_id: 'mx_a',
					machine: 'This Mac',
				}),
			),
		);
		expect(view.kind).toBe('stop');
		expect(view.title?.key).toBe('manager.spawn.titleStop');
		expect(view.chips).toEqual([{ key: 'irreversible', tone: 'danger' }]);
		expect(view.target).toEqual({ label: 'opencode', machine: 'This Mac' });
		expect(view.lines.map((l) => l.key)).toEqual([
			'target',
			'session',
			'effect',
			'undo',
		]);
	});

	it('names the requesting session for a send proposal', () => {
		const view = spawnView(
			wire(
				JSON.stringify({
					kind: 'send',
					session: 'codex',
					from: 'claude-1',
					text: 'run the tests',
					target_id: 'mx_a',
				}),
			),
		);
		expect(view.kind).toBe('send');
		expect(view.proposer).toEqual({ key: 'session', session: 'claude-1' });
		expect(view.title?.params).toEqual({ session: 'codex' });
		expect(view.chips).toEqual([{ key: 'foreign', tone: 'warn' }]);
		expect(view.lines.find((l) => l.key === 'text')?.value).toBe(
			'run the tests',
		);
	});

	it('falls back to the plain card for incomplete or unknown kinds', () => {
		for (const bad of [
			{ kind: 'stop', target_id: 'mx_a' },
			{ kind: 'send', session: 'a', target_id: 'mx_a' },
			{ kind: 'nuke', target_id: 'mx_a' },
		]) {
			const view = spawnView(wire(JSON.stringify(bad)));
			expect(view.structured).toBe(false);
			expect(view.title).toBeNull();
		}
	});

	it('keeps the request verbatim as the payload', () => {
		const req = request({ task: 'a  b' });
		expect(spawnView(wire(req)).payload).toBe(req);
	});

	it('flags a target mismatch and hidden characters', () => {
		const mismatch = { ...wire(request()), target_id: 'mx_other' };
		expect(spawnView(mismatch).chips).toEqual([
			{ key: 'mismatch', tone: 'danger' },
		]);
		const escaped = wire(request().replace('"claude"', '"cl\\u202eaude"'));
		expect(spawnView(escaped).chips).toEqual([
			{ key: 'hidden', tone: 'danger' },
		]);
		const plain = wire(JSON.stringify({ kind: 'nuke', target_id: 'other' }));
		expect(spawnView(plain).chips.map((c) => c.key)).toEqual(['mismatch']);
		const hidden = wire(request({ task: 'ok\u202Eevil' }));
		expect(spawnView(hidden).chips).toEqual([
			{ key: 'hidden', tone: 'danger' },
		]);
	});
});

describe('spawn card status', () => {
	it('reports outcomes for the resolved variant', () => {
		expect(
			spawnStatusText({ state: 'spawned', session: 's' }, 0, 0).outcome,
		).toBe('approved');
		expect(
			spawnStatusText({ state: 'refused', reason: 'untrusted' }, 0, 0).outcome,
		).toBe('rejected');
		expect(
			spawnStatusText({ state: 'refused', reason: 'untrusted' }, 0, 0, true)
				.outcome,
		).toBe('failed');
		expect(
			spawnStatusText({ state: 'refused', reason: 'expired' }, 0, 0).outcome,
		).toBe('expired');
		expect(spawnStatusText({ state: 'issued' }, 60_000, 0).outcome).toBe(
			'pending',
		);
	});

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
