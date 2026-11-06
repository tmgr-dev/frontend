import { createMover } from '../flow';
import { loadPersisted, savePersisted } from '../model';
import { harness, INVENTORY, publishedStatus, verifiedStatus } from './helpers';

const CODE = 'loc';

const skill = (slug: string) => ({
	slug,
	title: slug,
	when: '',
	markdown: `---\nslug: ${slug}\n---\nbody`,
	version: 1,
	updated_at: '2026-11-06T00:00:00Z',
});

const LIBRARY = {
	version: 1 as const,
	skills: [skill('a'), skill('b')],
	links: {},
	attachments: [
		{ persona_uuid: 'p-local', slug: 'a' },
		{ persona_uuid: 'p-cloud', slug: 'a' },
		{ persona_uuid: 'gone', slug: 'b' },
	],
};

const REPORT = [
	{
		slug: 'a',
		cloud_slug: 'a',
		status: 'created' as const,
		reason: null,
		attached: ['p-local'],
		skipped_attachments: [],
	},
];

const move = async (h: ReturnType<typeof harness>) => {
	const mover = createMover(h.deps);
	const dry = await mover.dryRun(CODE, 'Local');
	await mover.start(CODE, 'Local', dry, {
		cloudName: 'Cloud copy',
		personas: [{ local_uuid: 'p-local', action: 'create' }],
	});
	return mover;
};

const skillsOf = (h: ReturnType<typeof harness>) =>
	loadPersisted(h.storage, CODE).published?.skills;

describe('skills step after publish', () => {
	it('posts attached skills for the workspace personas and stores the report', async () => {
		const h = harness({ statuses: [publishedStatus()] });
		h.library.value = LIBRARY;
		h.api.importSkills.mockResolvedValue(REPORT);
		await move(h);
		expect(h.api.importSkills).toHaveBeenCalledTimes(1);
		expect(h.api.importSkills).toHaveBeenCalledWith('job-1', {
			skills: [
				{
					markdown: LIBRARY.skills[0].markdown,
					attach_to: ['p-local', 'p-cloud'],
				},
			],
		});
		expect(skillsOf(h)).toEqual({ state: 'done', report: REPORT, error: null });
	});

	it('also runs after a verified job is published by the server', async () => {
		const h = harness({ statuses: [verifiedStatus()] });
		h.library.value = LIBRARY;
		await move(h);
		expect(h.calls.indexOf('publish')).toBeGreaterThan(-1);
		expect(h.api.importSkills).toHaveBeenCalledTimes(1);
	});

	it('finishes with an empty report and no call without a library or attachments', async () => {
		const h = harness({ statuses: [publishedStatus()] });
		await move(h);
		expect(h.api.importSkills).not.toHaveBeenCalled();
		expect(skillsOf(h)).toEqual({ state: 'done', report: [], error: null });
	});

	it('fails with the message and retry runs only the skills step', async () => {
		const h = harness({ statuses: [publishedStatus()] });
		h.library.value = LIBRARY;
		h.api.importSkills.mockRejectedValueOnce(new Error('boom'));
		const mover = await move(h);
		expect(skillsOf(h)).toEqual({
			state: 'failed',
			report: null,
			error: 'boom',
		});
		const calls = h.calls.length;
		h.api.importSkills.mockResolvedValueOnce(REPORT);
		await mover.retrySkills(CODE);
		expect(skillsOf(h)).toMatchObject({ state: 'done', report: REPORT });
		expect(h.calls.slice(calls)).toEqual([]);
		expect(h.api.createJob).toHaveBeenCalledTimes(1);
		expect(h.api.importSkills).toHaveBeenCalledTimes(2);
	});

	it('retry does nothing once the step is done', async () => {
		const h = harness({ statuses: [publishedStatus()] });
		h.library.value = LIBRARY;
		const mover = await move(h);
		await mover.retrySkills(CODE);
		expect(h.api.importSkills).toHaveBeenCalledTimes(1);
	});

	it('resume on app start finishes a pending step for the owner', async () => {
		const h = harness({ statuses: [publishedStatus()] });
		h.library.value = LIBRARY;
		await move(h);
		const state = loadPersisted(h.storage, CODE);
		state.published!.skills = { state: 'pending', report: null, error: null };
		savePersisted(h.storage, CODE, state);
		await createMover(h.deps).resume(CODE);
		expect(h.api.importSkills).toHaveBeenCalledTimes(2);
		expect(skillsOf(h)?.state).toBe('done');
	});

	it('does nothing for another account', async () => {
		const h = harness({ statuses: [publishedStatus()] });
		h.library.value = LIBRARY;
		const mover = await move(h);
		const state = loadPersisted(h.storage, CODE);
		state.published!.skills = { state: 'failed', report: null, error: 'x' };
		savePersisted(h.storage, CODE, state);
		h.user.id = 8;
		await mover.resume(CODE);
		await mover.retrySkills(CODE);
		expect(h.api.importSkills).toHaveBeenCalledTimes(1);
		expect(skillsOf(h)?.state).toBe('failed');
	});

	it('does nothing for a move published before this version', async () => {
		const h = harness({ statuses: [publishedStatus()] });
		h.library.value = LIBRARY;
		const mover = await move(h);
		const state = loadPersisted(h.storage, CODE);
		delete state.published!.skills;
		savePersisted(h.storage, CODE, state);
		await mover.resume(CODE);
		await mover.retrySkills(CODE);
		expect(h.api.importSkills).toHaveBeenCalledTimes(1);
		expect(skillsOf(h)).toBeUndefined();
	});
});

describe('dry run skills count', () => {
	it('counts library skills attached to workspace personas', async () => {
		const h = harness();
		h.library.value = LIBRARY;
		const dry = await createMover(h.deps).dryRun(CODE, 'Local');
		expect(dry.skills).toBe(1);
		expect(INVENTORY.personas.map((p) => p.uuid)).toContain('p-local');
	});

	it('is zero when signed out or the library cannot be read', async () => {
		const h = harness({ userId: null });
		h.library.value = LIBRARY;
		expect((await createMover(h.deps).dryRun(CODE, 'Local')).skills).toBe(0);
		const failing = harness();
		failing.deps.skillLibrary = async () => {
			throw new Error('no file');
		};
		expect((await createMover(failing.deps).dryRun(CODE, 'Local')).skills).toBe(
			0,
		);
	});
});
