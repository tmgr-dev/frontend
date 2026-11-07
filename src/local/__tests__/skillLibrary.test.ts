import { createHash } from 'crypto';
import { EMPTY_FILES_SHA256, type SkillFileSet } from '../skillFileSet';
import {
	attach,
	deleteSkill,
	detach,
	emptyLibrary,
	exportProjection,
	importSkills,
	type LibraryFile,
	mergeSkills,
	personaSkillBody,
	personaSkillFile,
	personaSkillIndex,
	putSkill,
	SkillLibraryError,
	validateLibrary,
	validateLibraryReport,
} from '../skillLibrary';

const NOW = '2026-11-06T10:00:00.000Z';
const LATER = '2026-11-07T10:00:00.000Z';
const PERSONA = 'persona-1';

const md = (slug: string, title = `Title ${slug}`, body = 'Body text') =>
	`---\nslug: ${slug}\ntitle: ${title}\nwhen: Use when ${slug}\nactions: []\n---\n${body}`;

const withSkills = (...slugs: string[]): LibraryFile =>
	slugs.reduce(
		(lib, slug) => putSkill(lib, slug, md(slug), NOW).library,
		emptyLibrary(),
	);

const ref = (slug: string, version = 1) => ({
	slug,
	title: `T ${slug}`,
	when: `w ${slug}`,
	version,
});

describe('emptyLibrary', () => {
	it('returns a version 1 library with no skills, links or attachments', () => {
		expect(emptyLibrary()).toEqual({
			version: 1,
			skills: [],
			links: {},
			attachments: [],
		});
	});

	it('returns a fresh object on every call', () => {
		const first = emptyLibrary();
		first.skills.push({} as never);
		expect(emptyLibrary().skills).toEqual([]);
	});
});

describe('validateLibrary', () => {
	it('accepts a well-formed library and derives title and when', () => {
		const lib = withSkills('alpha');
		const validated = validateLibrary(JSON.parse(JSON.stringify(lib)));
		expect(validated.skills).toEqual(lib.skills);
		expect(validated.links).toEqual({});
		expect(validated.attachments).toEqual([]);
	});

	it('tolerates missing links and attachments', () => {
		const validated = validateLibrary({
			version: 1,
			skills: [{ ...withSkills('alpha').skills[0] }],
		});
		expect(validated.links).toEqual({});
		expect(validated.attachments).toEqual([]);
	});

	it('rejects a wrong version', () => {
		expect(() => validateLibrary({ version: 2, skills: [] })).toThrow(
			SkillLibraryError,
		);
	});

	it('rejects non-object input and a non-array skills field', () => {
		expect(() => validateLibrary(null)).toThrow(SkillLibraryError);
		expect(() => validateLibrary({ version: 1, skills: {} })).toThrow(
			SkillLibraryError,
		);
	});

	it('rejects more than the shared skill limit', () => {
		const skills = Array.from({ length: 101 }, (_, i) => ({
			...withSkills(`s${i}`).skills[0],
			slug: `s${i}`,
		}));
		expect(() => validateLibrary({ version: 1, skills })).toThrow(
			/at most 100/,
		);
	});

	it('skips a bad skill with its links and attachments and keeps the rest', () => {
		const good = withSkills('alpha').skills[0];
		const link = {
			slug: 'bad',
			sourceHash: 'h',
			libraryVersion: 1,
			importedAt: NOW,
		};
		const { library, skipped } = validateLibraryReport({
			version: 1,
			skills: [
				good,
				{ ...good, slug: 'bad', markdown: 'no front matter' },
				{ ...good, slug: 'zero', version: 0 },
				{ ...good, slug: 'Bad Slug' },
				{ ...good },
			],
			links: { k: link },
			attachments: [
				{ persona_uuid: PERSONA, slug: 'bad' },
				{ persona_uuid: PERSONA, slug: 'alpha' },
			],
		});
		expect(library.skills.map((s) => s.slug)).toEqual(['alpha']);
		expect(skipped.map((s) => s.slug)).toEqual([
			'bad',
			'zero',
			'Bad Slug',
			'alpha',
		]);
		expect(library.links).toEqual({});
		expect(library.attachments).toEqual([
			{ persona_uuid: PERSONA, slug: 'alpha' },
		]);
		expect(
			validateLibrary({ version: 1, skills: [{ ...good, version: 1.5 }] })
				.skills,
		).toEqual([]);
	});

	it('keeps unknown top-level and per-skill fields through a write', () => {
		const good = { ...withSkills('alpha').skills[0], future: { a: 1 } };
		const lib = validateLibrary({ version: 1, skills: [good], theme: 'x' });
		expect(lib.theme).toBe('x');
		expect(lib.skills[0].future).toEqual({ a: 1 });
		const next = putSkill(lib, 'alpha', md('alpha', 'New'), LATER).library;
		expect(next.theme).toBe('x');
		expect(next.skills[0].future).toEqual({ a: 1 });
		expect(attach(next, PERSONA, 'alpha').theme).toBe('x');
	});

	it('rejects malformed links', () => {
		expect(() =>
			validateLibrary({ version: 1, skills: [], links: { key: { slug: 1 } } }),
		).toThrow(SkillLibraryError);
	});

	it('keeps well-formed links', () => {
		const entry = {
			slug: 'alpha',
			sourceHash: 'h',
			libraryVersion: 1,
			importedAt: NOW,
		};
		const validated = validateLibrary({
			version: 1,
			skills: [],
			links: { 'scan-key': entry },
		});
		expect(validated.links).toEqual({ 'scan-key': entry });
	});

	it('rejects malformed attachments and a persona over the attach limit', () => {
		expect(() =>
			validateLibrary({ version: 1, skills: [], attachments: [{ slug: 'x' }] }),
		).toThrow(SkillLibraryError);
		const attachments = Array.from({ length: 21 }, (_, i) => ({
			persona_uuid: PERSONA,
			slug: `s${i}`,
		}));
		expect(() =>
			validateLibrary({ version: 1, skills: [], attachments }),
		).toThrow(/at most 20/);
	});
});

describe('putSkill', () => {
	it('creates a new skill at version 1 with title, when and updated_at from the front matter', () => {
		const { library, skill } = putSkill(
			emptyLibrary(),
			'alpha',
			md('alpha'),
			NOW,
		);
		expect(skill).toEqual({
			slug: 'alpha',
			title: 'Title alpha',
			when: 'Use when alpha',
			markdown: md('alpha'),
			version: 1,
			updated_at: NOW,
		});
		expect(library.skills).toEqual([skill]);
	});

	it('bumps the version and updated_at only when markdown changes', () => {
		const first = putSkill(emptyLibrary(), 'alpha', md('alpha'), NOW).library;
		const same = putSkill(first, 'alpha', md('alpha'), LATER);
		expect(same.library).toBe(first);
		expect(same.skill.version).toBe(1);
		expect(same.skill.updated_at).toBe(NOW);

		const changed = putSkill(first, 'alpha', md('alpha', 'New title'), LATER);
		expect(changed.skill.version).toBe(2);
		expect(changed.skill.updated_at).toBe(LATER);
		expect(changed.skill.title).toBe('New title');
		expect(changed.library.skills).toHaveLength(1);
	});

	it('does not mutate the input library', () => {
		const lib = withSkills('alpha');
		const snapshot = JSON.stringify(lib);
		putSkill(lib, 'beta', md('beta'), NOW);
		putSkill(lib, 'alpha', md('alpha', 'Other'), NOW);
		expect(JSON.stringify(lib)).toBe(snapshot);
	});

	it('rejects markdown whose slug does not match the given slug', () => {
		expect(() => putSkill(emptyLibrary(), 'alpha', md('beta'), NOW)).toThrow(
			/must match the URL slug/,
		);
	});

	it('rejects an invalid slug', () => {
		expect(() =>
			putSkill(emptyLibrary(), 'Bad_Slug', md('Bad_Slug'), NOW),
		).toThrow(SkillLibraryError);
	});

	it('rejects markdown over the body byte limit', () => {
		const big = md('alpha', 'Title alpha', 'x'.repeat(16385));
		expect(() => putSkill(emptyLibrary(), 'alpha', big, NOW)).toThrow(
			/must not exceed 16384 bytes/,
		);
	});

	it('rejects a new skill once the library holds 100 skills', () => {
		let lib = emptyLibrary();
		for (let i = 0; i < 100; i++) {
			lib = putSkill(lib, `s${i}`, md(`s${i}`), NOW).library;
		}
		expect(() => putSkill(lib, 'extra', md('extra'), NOW)).toThrow(
			/at most 100/,
		);
	});

	it('still updates an existing skill when the library is at the limit', () => {
		let lib = emptyLibrary();
		for (let i = 0; i < 100; i++) {
			lib = putSkill(lib, `s${i}`, md(`s${i}`), NOW).library;
		}
		const updated = putSkill(lib, 's0', md('s0', 'Renamed'), LATER);
		expect(updated.skill.version).toBe(2);
		expect(updated.library.skills).toHaveLength(100);
	});
});

describe('deleteSkill', () => {
	it('removes the skill and its attachments and links for that slug', () => {
		const base = withSkills('alpha', 'beta');
		const lib: LibraryFile = {
			...base,
			attachments: [
				{ persona_uuid: PERSONA, slug: 'alpha' },
				{ persona_uuid: PERSONA, slug: 'beta' },
			],
			links: {
				'k-alpha': {
					slug: 'alpha',
					sourceHash: 'h',
					libraryVersion: 1,
					importedAt: NOW,
				},
				'k-beta': {
					slug: 'beta',
					sourceHash: 'h',
					libraryVersion: 1,
					importedAt: NOW,
				},
			},
		};
		const result = deleteSkill(lib, 'alpha');
		expect(result.skills.map((s) => s.slug)).toEqual(['beta']);
		expect(result.attachments).toEqual([
			{ persona_uuid: PERSONA, slug: 'beta' },
		]);
		expect(Object.keys(result.links)).toEqual(['k-beta']);
	});

	it('is a no-op for an unknown slug', () => {
		const lib = withSkills('alpha');
		expect(deleteSkill(lib, 'missing')).toEqual(lib);
	});
});

describe('attach and detach', () => {
	it('attaches an existing skill to a persona', () => {
		const lib = attach(withSkills('alpha'), PERSONA, 'alpha');
		expect(lib.attachments).toEqual([{ persona_uuid: PERSONA, slug: 'alpha' }]);
	});

	it('is idempotent when the skill is already attached', () => {
		const once = attach(withSkills('alpha'), PERSONA, 'alpha');
		expect(attach(once, PERSONA, 'alpha')).toBe(once);
	});

	it('rejects attaching a skill that does not exist', () => {
		expect(() => attach(withSkills('alpha'), PERSONA, 'missing')).toThrow(
			/does not exist/,
		);
	});

	it('enforces the per-persona attach limit of 20 but not across personas', () => {
		let lib = emptyLibrary();
		for (let i = 0; i < 21; i++) {
			lib = putSkill(lib, `s${i}`, md(`s${i}`), NOW).library;
		}
		for (let i = 0; i < 20; i++) {
			lib = attach(lib, PERSONA, `s${i}`);
		}
		expect(() => attach(lib, PERSONA, 's20')).toThrow(/at most 20/);
		expect(attach(lib, 'persona-2', 's20').attachments).toHaveLength(21);
	});

	it('detaches a skill and is a no-op when it was not attached', () => {
		const attached = attach(withSkills('alpha', 'beta'), PERSONA, 'alpha');
		const detached = detach(attached, PERSONA, 'alpha');
		expect(detached.attachments).toEqual([]);
		expect(detach(detached, PERSONA, 'alpha')).toBe(detached);
	});

	it('detaches only the given persona', () => {
		let lib = attach(withSkills('alpha'), PERSONA, 'alpha');
		lib = attach(lib, 'persona-2', 'alpha');
		expect(detach(lib, PERSONA, 'alpha').attachments).toEqual([
			{ persona_uuid: 'persona-2', slug: 'alpha' },
		]);
	});
});

describe('mergeSkills', () => {
	it('lets own skills win on a slug clash and hides the overridden shared skill', () => {
		const merged = mergeSkills(
			[ref('beta', 3)],
			[ref('alpha', 1), ref('beta', 9)],
		);
		expect(merged).toEqual([
			{ ...ref('alpha', 1), source: 'shared' },
			{ ...ref('beta', 3), source: 'own' },
		]);
	});

	it('sorts by slug regardless of input order', () => {
		const merged = mergeSkills(
			[ref('zeta'), ref('mid')],
			[ref('alpha'), ref('omega')],
		);
		expect(merged.map((s) => s.slug)).toEqual([
			'alpha',
			'mid',
			'omega',
			'zeta',
		]);
	});

	it('returns an empty list for empty inputs', () => {
		expect(mergeSkills([], [])).toEqual([]);
	});

	it('keeps only slug, title, when, version and source', () => {
		const [entry] = mergeSkills([], [{ ...ref('alpha'), extra: 'x' } as never]);
		expect(Object.keys(entry).sort()).toEqual(
			['slug', 'source', 'title', 'version', 'when'].sort(),
		);
	});
});

describe('personaSkillIndex', () => {
	it('lists only skills attached to the persona as shared', () => {
		let lib = withSkills('alpha', 'beta', 'gamma');
		lib = attach(lib, PERSONA, 'gamma');
		lib = attach(lib, PERSONA, 'alpha');
		lib = attach(lib, 'persona-2', 'beta');
		expect(personaSkillIndex(lib, PERSONA)).toEqual([
			{
				slug: 'alpha',
				title: 'Title alpha',
				when: 'Use when alpha',
				version: 1,
				source: 'shared',
			},
			{
				slug: 'gamma',
				title: 'Title gamma',
				when: 'Use when gamma',
				version: 1,
				source: 'shared',
			},
		]);
	});

	it('returns an empty index for a persona with no attachments', () => {
		expect(personaSkillIndex(withSkills('alpha'), PERSONA)).toEqual([]);
	});
});

describe('personaSkillBody', () => {
	it('returns the markdown body of an attached skill', () => {
		const lib = attach(withSkills('alpha'), PERSONA, 'alpha');
		expect(personaSkillBody(lib, PERSONA, 'alpha')).toEqual({
			slug: 'alpha',
			body: md('alpha'),
			version: 1,
			source: 'shared',
			files: [],
			files_sha256: EMPTY_FILES_SHA256,
		});
	});

	it('returns null when the skill is not attached to that persona', () => {
		const lib = attach(withSkills('alpha'), PERSONA, 'alpha');
		expect(personaSkillBody(lib, 'persona-2', 'alpha')).toBeNull();
		expect(personaSkillBody(lib, PERSONA, 'missing')).toBeNull();
	});
});

describe('exportProjection', () => {
	it('exports version and skills only, dropping links and attachments', () => {
		const lib: LibraryFile = {
			...attach(withSkills('alpha'), PERSONA, 'alpha'),
			links: {
				k: {
					slug: 'alpha',
					sourceHash: 'h',
					libraryVersion: 1,
					importedAt: NOW,
				},
			},
		};
		expect(exportProjection(lib)).toEqual({
			version: 1,
			skills: lib.skills,
		});
		expect(Object.keys(exportProjection(lib)).sort()).toEqual([
			'skills',
			'version',
		]);
	});
});

describe('importSkills', () => {
	const incoming = (...skills: unknown[]) => ({ version: 1, skills });

	it('imports new skills at version 1 with derived title and when', () => {
		const result = importSkills(
			emptyLibrary(),
			incoming({ slug: 'alpha', markdown: md('alpha'), title: 'ignored' }),
			NOW,
		);
		expect(result.imported).toEqual(['alpha']);
		expect(result.skipped).toEqual([]);
		expect(result.library.skills[0]).toEqual({
			slug: 'alpha',
			title: 'Title alpha',
			when: 'Use when alpha',
			markdown: md('alpha'),
			version: 1,
			updated_at: NOW,
		});
	});

	it('skips a slug that already exists with reason exists and keeps the existing skill', () => {
		const lib = withSkills('alpha');
		const result = importSkills(
			lib,
			incoming({ slug: 'alpha', markdown: md('alpha', 'Other') }),
			LATER,
		);
		expect(result.imported).toEqual([]);
		expect(result.skipped).toEqual([{ slug: 'alpha', reason: 'exists' }]);
		expect(result.library.skills).toEqual(lib.skills);
	});

	it('treats a duplicate slug within one import as exists after the first is imported', () => {
		const result = importSkills(
			emptyLibrary(),
			incoming(
				{ slug: 'alpha', markdown: md('alpha') },
				{ slug: 'alpha', markdown: md('alpha', 'Second') },
			),
			NOW,
		);
		expect(result.imported).toEqual(['alpha']);
		expect(result.skipped).toEqual([{ slug: 'alpha', reason: 'exists' }]);
	});

	it('skips invalid entries with a reason and continues', () => {
		const result = importSkills(
			emptyLibrary(),
			incoming(
				{ slug: 'bad', markdown: 'no front matter' },
				{ markdown: md('nope') },
				'not an object',
				{ slug: 'good', markdown: md('good') },
			),
			NOW,
		);
		expect(result.imported).toEqual(['good']);
		expect(result.skipped).toHaveLength(3);
		expect(result.skipped[0].slug).toBe('bad');
		expect(result.skipped[0].reason).toMatch(/front-matter/);
		expect(result.skipped[1]).toEqual({ slug: '', reason: 'invalid entry' });
		expect(result.skipped[2]).toEqual({ slug: '', reason: 'invalid entry' });
	});

	it('respects the 100-skill library limit and reports the rest as skipped', () => {
		let lib = emptyLibrary();
		for (let i = 0; i < 99; i++) {
			lib = putSkill(lib, `s${i}`, md(`s${i}`), NOW).library;
		}
		const result = importSkills(
			lib,
			incoming(
				{ slug: 'a', markdown: md('a') },
				{ slug: 'b', markdown: md('b') },
			),
			NOW,
		);
		expect(result.imported).toEqual(['a']);
		expect(result.library.skills).toHaveLength(100);
		expect(result.skipped).toEqual([
			{ slug: 'b', reason: 'library holds at most 100 skills' },
		]);
	});

	it('rejects an import file that is not {version: 1, skills: []}', () => {
		expect(() =>
			importSkills(emptyLibrary(), { version: 2, skills: [] }, NOW),
		).toThrow(SkillLibraryError);
		expect(() => importSkills(emptyLibrary(), { version: 1 }, NOW)).toThrow(
			SkillLibraryError,
		);
	});
});

const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const fileSetOf = (...pairs: [string, string][]): SkillFileSet => {
	const files = pairs.map(([path, content]) => ({
		path,
		content,
		size: Buffer.byteLength(content),
		sha256: sha(content),
	}));
	const joined = [...files]
		.sort((a, b) => (a.path < b.path ? -1 : 1))
		.map((f) => `${f.path}\0${f.sha256}\n`)
		.join('');
	return { files, sha256: sha(joined) };
};

describe('skill files in the local library', () => {
	const set = fileSetOf(['run.sh', 'echo hi'], ['refs/g.md', 'guide']);

	it('stores body, files and hash in one skill record', () => {
		const { skill } = putSkill(emptyLibrary(), 'kit', md('kit'), NOW, set);
		expect(skill.files).toEqual(set.files);
		expect(skill.files_sha256).toBe(set.sha256);
		expect(skill.version).toBe(1);
	});

	it('keeps the files when only the body changes and the body when only files change', () => {
		const first = putSkill(emptyLibrary(), 'kit', md('kit'), NOW, set).library;
		const body = putSkill(first, 'kit', md('kit', 'T', 'new'), LATER);
		expect(body.skill.files).toEqual(set.files);
		expect(body.skill.version).toBe(2);
		const next = fileSetOf(['run.sh', 'changed']);
		const filesOnly = putSkill(first, 'kit', md('kit'), LATER, next);
		expect(filesOnly.skill.files).toEqual(next.files);
		expect(filesOnly.skill.version).toBe(1);
		expect(filesOnly.library).not.toBe(first);
	});

	it('is a no-op for identical body and files, and an empty set clears the files', () => {
		const first = putSkill(emptyLibrary(), 'kit', md('kit'), NOW, set).library;
		expect(putSkill(first, 'kit', md('kit'), LATER, set).library).toBe(first);
		const cleared = putSkill(first, 'kit', md('kit'), LATER, fileSetOf());
		expect(cleared.skill.files).toBeUndefined();
		expect(cleared.skill.files_sha256).toBeUndefined();
	});

	it('rejects bad paths, case clashes, NUL, size and count limits', () => {
		const put = (s: SkillFileSet) =>
			putSkill(emptyLibrary(), 'kit', md('kit'), NOW, s);
		for (const bad of [
			'../x',
			'/a',
			'a/./b',
			'.git/c',
			'a\\b',
			'SKILL.md',
			'skill.md',
			'a/b/c/d/e/f',
		]) {
			expect(() => put(fileSetOf([bad, 'x']))).toThrow(SkillLibraryError);
		}
		expect(() => put(fileSetOf(['A.md', '1'], ['a.md', '2']))).toThrow(/case/);
		expect(() => put(fileSetOf(['f', 'a\0b']))).toThrow(SkillLibraryError);
		expect(() => put(fileSetOf(['f', 'a'.repeat(64 * 1024 + 1)]))).toThrow(
			/64 KiB/,
		);
		const many = Array.from(
			{ length: 51 },
			(_, i) => [`f${i}`, 'x'] as [string, string],
		);
		expect(() => put(fileSetOf(...many))).toThrow(/at most 50/);
		const heavy = ['a', 'b', 'c', 'd', 'e'].map(
			(n) => [n, 'a'.repeat(60 * 1024)] as [string, string],
		);
		expect(() => put(fileSetOf(...heavy))).toThrow(/256 KiB/);
		expect(
			put(fileSetOf(['references/SKILL.md', 'ok'])).skill.files,
		).toHaveLength(1);
	});

	it('drops a stored skill with tampered files when the library is read', () => {
		const lib = putSkill(emptyLibrary(), 'kit', md('kit'), NOW, set).library;
		const bad = {
			...lib,
			skills: [
				{ ...lib.skills[0], files: [{ ...set.files[0], path: '../x' }] },
			],
		};
		const report = validateLibraryReport(bad);
		expect(report.library.skills).toEqual([]);
		expect(report.skipped[0].slug).toBe('kit');
		expect(validateLibraryReport(lib).library.skills[0].files).toEqual(
			set.files,
		);
	});

	it('serves files to the persona the skill is attached to only', () => {
		const lib = attach(
			putSkill(emptyLibrary(), 'kit', md('kit'), NOW, set).library,
			PERSONA,
			'kit',
		);
		expect(personaSkillBody(lib, PERSONA, 'kit')).toMatchObject({
			files: [
				{ path: 'run.sh', size: 7 },
				{ path: 'refs/g.md', size: 5 },
			],
			files_sha256: set.sha256,
		});
		expect(personaSkillFile(lib, PERSONA, 'kit', 'run.sh')).toEqual({
			slug: 'kit',
			path: 'run.sh',
			content: 'echo hi',
			size: 7,
			sha256: sha('echo hi'),
		});
		expect(personaSkillFile(lib, PERSONA, 'kit', 'nope.sh')).toBeNull();
		expect(personaSkillFile(lib, 'persona-2', 'kit', 'run.sh')).toBeNull();
	});

	it('carries files through the export and import file', () => {
		const lib = putSkill(emptyLibrary(), 'kit', md('kit'), NOW, set).library;
		const out = importSkills(emptyLibrary(), exportProjection(lib), LATER);
		expect(out.imported).toEqual(['kit']);
		expect(out.library.skills[0].files).toEqual(set.files);
	});
});
