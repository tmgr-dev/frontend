import { SHORTCUT_ACTIONS } from '@/utils/desktopShortcuts';
import { getTour } from '../catalog';
import { DESKTOP_RELEASE_TOURS, desktopTours } from '../catalog/desktop';
import { KNOWN_EVENTS } from '../events';
import { tokensIn } from '../keycaps';

const S8_IDS = [
	'desktop.quick-add',
	'desktop.capture',
	'desktop.menu-bar-timer',
	'desktop.windows',
	'desktop.local-workspace',
	'desktop.backups',
	'desktop.local-agent',
	'desktop.plugins',
	'desktop.whats-new',
	'desktop.move-to-cloud',
];
const words = (text: string) => text.trim().split(/\s+/).length;
const tour = (id: string) => getTour(id)!;

describe('S8 desktop catalog', () => {
	it('registers D1-D10 in order with a hub trigger', () => {
		expect(desktopTours.map((t) => t.id)).toEqual(S8_IDS);
		for (const id of S8_IDS) {
			expect(tour(id).group).toBe('desktop');
			expect(tour(id).since).toBe('3.4.158');
			expect(tour(id).triggers).toEqual(
				expect.arrayContaining([{ kind: 'hub' }]),
			);
			expect(tour(id).steps.map((s) => s.id)).toEqual(
				tour(id).steps.map((_, i) => `s${i + 1}`),
			);
		}
	});

	it('every platform variant is at most 25 words', () => {
		for (const t of desktopTours)
			for (const step of t.steps)
				for (const text of [
					step.body,
					step.mobileBody,
					step.bodyByPlatform?.macos,
					step.bodyByPlatform?.other,
				])
					if (text)
						expect(`${t.id}/${step.id}: ${words(text)}`).toMatch(
							/: ([1-9]|1\d|2[0-5])$/,
						);
	});

	it('uses only known tokens', () => {
		const known = new Set(['tray', ...SHORTCUT_ACTIONS.map((a) => `key:${a}`)]);
		for (const t of desktopTours)
			for (const step of t.steps)
				for (const text of [
					step.body,
					step.bodyByPlatform?.macos,
					step.bodyByPlatform?.other,
				])
					for (const token of tokensIn(text ?? ''))
						expect(known.has(token)).toBe(true);
	});

	it('requirements match the plan', () => {
		expect(tour('desktop.local-agent').requires).toMatchObject({
			capability: 'localAccess',
			modules: ['personas', 'agent_work'],
		});
		expect(tour('desktop.local-workspace').requires).toMatchObject({
			account: 'local',
		});
		expect(tour('desktop.backups').requires?.platforms).toEqual([
			'desktop_local',
		]);
		expect(tour('desktop.move-to-cloud').requires).toMatchObject({
			account: 'cloud',
			platforms: ['desktop'],
		});
		for (const t of desktopTours)
			expect(t.requires?.platforms).not.toContain('web');
	});

	it('quick add waits for a quick-add task', () => {
		expect(tour('desktop.quick-add').steps[1].waitFor).toEqual({
			event: 'task.created',
			where: { source: 'quick_add' },
		});
	});

	it('local agent waits for the first local call and skips when already connected', () => {
		const s3 = tour('desktop.local-agent').steps[2];
		expect(s3).toMatchObject({
			waitFor: { event: 'agent.first_local_call' },
			alreadyDone: 'local_agent_connected',
		});
	});

	it('move to cloud listens for migration events', () => {
		const t = tour('desktop.move-to-cloud');
		expect(t.steps[0]).toMatchObject({
			anchor: 'settings.data-move',
			waitFor: { event: 'migration.started' },
		});
		expect(t.steps[1].waitFor).toEqual({ event: 'migration.completed' });
		for (const e of ['migration.started', 'migration.completed'])
			expect(KNOWN_EVENTS).toContain(e);
	});

	it('release map points at real tours', () => {
		expect(Object.keys(DESKTOP_RELEASE_TOURS)).toEqual(['0.9.37']);
		for (const id of DESKTOP_RELEASE_TOURS['0.9.37'])
			expect(getTour(id)).toBeDefined();
	});
});
