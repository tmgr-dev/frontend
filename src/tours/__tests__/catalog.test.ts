import { readdirSync, readFileSync, statSync } from 'fs';
import { join } from 'path';
import { KNOWN_EVENTS } from '../events';
import { MODULES } from '@/local/modulesCatalog';
import { getTour, TOURS } from '../catalog';

const ANCHORLESS_WAITS = new Set(['desktop.quick-add/s2', 'desktop.move-to-cloud/s2']);

const words = (text: string) => text.trim().split(/\s+/).length;

const walk = (dir: string): string[] =>
	readdirSync(dir).flatMap((name) => {
		const path = join(dir, name);
		return statSync(path).isDirectory() ? walk(path) : [path];
	});

describe('catalog lint', () => {
	it('has the S2 tours', () => {
		expect(TOURS.map((t) => t.id)).toEqual(
			expect.arrayContaining([
				'start.connect-agent',
				'start.first-task',
				'start.hub',
				'start.nav',
				'tasks.anatomy',
				'tasks.archive',
				'tasks.board-move',
				'tasks.comments-files',
				'time.timer',
			]),
		);
		expect(getTour('time.timer')?.steps).toHaveLength(4);
	});

	it('the connect-agent tour waits on the first MCP call without completing on it', () => {
		const tour = getTour('start.connect-agent')!;
		expect(tour.completeOn).toBeUndefined();
		expect(tour.requires).toMatchObject({ modules: ['mcp'], account: 'cloud' });
		expect(tour.requires?.platforms).not.toContain('desktop_local');
		expect(tour.triggers).toEqual(expect.arrayContaining([{ kind: 'first_run', route: 'agent' }, { kind: 'hub' }]));
		expect(tour.steps.map((s) => s.route?.name)).toEqual(Array(4).fill('WelcomeSetupAgent'));
		expect(tour.steps[2]).toMatchObject({
			type: 'wait',
			waitFor: { event: 'agent.first_mcp_call' },
			alreadyDone: 'mcp_first_call_at',
		});
		expect(tour.steps[0].skipWhen).toBe('mcp_first_call_at');
		expect(tour.steps[1].skipWhen).toBe('mcp_first_call_at');
		expect(tour.steps[3].type).toBe('tip');
	});

	it('tour ids and step ids are unique', () => {
		expect(new Set(TOURS.map((t) => t.id)).size).toBe(TOURS.length);
		for (const tour of TOURS) {
			expect(new Set(tour.steps.map((s) => s.id)).size).toBe(tour.steps.length);
			tour.steps.forEach((s) => expect(s.id.length).toBeLessThanOrEqual(48));
			expect(tour.steps.map((s) => s.id)).toEqual(tour.steps.map((_, i) => `s${i + 1}`));
			tour.steps.forEach((s) => expect(s.id).toMatch(/^s\d+$/));
			expect(tour.id.length).toBeLessThanOrEqual(64);
		}
	});

	it('copy is at most 25 words', () => {
		for (const tour of TOURS)
			for (const step of tour.steps) {
				for (const text of [step.title, step.body, step.mobileBody].filter(Boolean) as string[])
					expect(`${tour.id}/${step.id}: ${words(text)}`).toMatch(/: ([1-9]|1\d|2[0-5])$/);
			}
	});

	const isKnownEvent = (event: unknown) => {
		if (typeof event === 'string' && event.startsWith('module.enabled:'))
			return MODULES.some((m) => m.key === event.slice('module.enabled:'.length));
		return (KNOWN_EVENTS as readonly string[]).includes(event as string);
	};

	it('wait steps name a known event; other steps do not', () => {
		for (const tour of TOURS)
			for (const step of tour.steps) {
				if (step.type === 'wait') {
					expect(`${tour.id}/${step.id}: ${isKnownEvent(step.waitFor?.event)}`).toMatch(/: true$/);
				} else expect(step.waitFor).toBeUndefined();
				if (step.alreadyDone) expect(step.type).toBe('wait');
			}
	});

	it('completeOn, event triggers and where refer to known events', () => {
		for (const tour of TOURS) {
			if (tour.completeOn) expect(`${tour.id}: ${isKnownEvent(tour.completeOn)}`).toMatch(/: true$/);
			for (const trigger of tour.triggers)
				if (trigger.kind === 'event') expect(isKnownEvent(trigger.event)).toBe(true);
		}
	});

	it('module keys exist in the module catalog', () => {
		const keys = new Set(MODULES.map((m) => m.key));
		for (const tour of TOURS) {
			(tour.requires?.modules ?? []).forEach((k) => expect(keys.has(k)).toBe(true));
			tour.steps.forEach((s) => (s.requiresModules ?? []).forEach((k) => expect(keys.has(k)).toBe(true)));
		}
	});

	it('non-modal steps have an anchor, every tour has a trigger and a hub entry', () => {
		for (const tour of TOURS) {
			expect(tour.triggers.some((t) => t.kind === 'hub')).toBe(true);
			expect(tour.steps.length).toBeGreaterThan(0);
			for (const step of tour.steps) if (step.type !== 'modal' && !ANCHORLESS_WAITS.has(`${tour.id}/${step.id}`)) expect(step.anchor).toBeTruthy();
		}
	});

	it('every nav.* step works under 640px', () => {
		for (const tour of TOURS)
			for (const step of tour.steps)
				if (step.anchor?.startsWith('nav.') && step.anchor !== 'nav.create-task' && step.anchor !== 'nav.active-timers')
					expect(step.mobileAnchor || step.mobileBody).toBeTruthy();
	});

	it('sidebar page anchors work under 640px', () => {
		for (const tour of TOURS)
			for (const step of tour.steps)
				if (step.anchor === 'pages.tree' || step.anchor === 'pages.type-picker')
					expect(step.mobileAnchor || step.mobileBody).toBeTruthy();
	});

	it('tours never import the local modules snapshot', () => {
		const files = walk(join(__dirname, '..')).filter(
			(f) => /\.(ts|vue)$/.test(f) && !f.includes('__tests__'),
		);
		expect(files.length).toBeGreaterThan(5);
		for (const file of files) expect(readFileSync(file, 'utf8')).not.toMatch(/modulesCatalog/);
	});
});
