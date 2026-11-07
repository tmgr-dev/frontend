import {
	EMPTY_FILES_SHA256 as EMPTY,
	type ScanResult,
	type ScanSkill,
} from '@/utils/skillImport';
import { emptyLibrary, type LibraryFile } from '../skillLibrary';
import { linkedUpdateStatuses } from '../skillUpdateStatus';

const skill = (over: Partial<ScanSkill> = {}): ScanSkill => ({
	key: 'claude:personal:review',
	dir_name: 'review',
	path: '~/.claude/skills/review',
	content: '---\nname: review\ndescription: Review code\n---\n# Review\nBody\n',
	hash: 'h1',
	size: 60,
	too_large: false,
	secret: false,
	secret_reason: null,
	symlink_target: null,
	files: [],
	files_sha256: EMPTY,
	files_blocked: null,
	files_truncated: false,
	...over,
});

const scanOf = (...skills: ScanSkill[]): ScanResult => ({
	sources: [
		{
			id: 'claude',
			label: 'Claude Code',
			packages: [
				{ id: 'personal', name: 'Personal skills', version: null, skills },
			],
		},
	],
	skipped: [],
});

const libOf = (version: number): LibraryFile => ({
	...emptyLibrary(),
	skills: [
		{
			slug: 'review',
			title: 'Review',
			when: 'Use when: x',
			markdown: 'm',
			version,
			updated_at: 'now',
		},
	],
	links: {
		'claude:personal:review': {
			slug: 'review',
			sourceHash: 'h1',
			libraryVersion: 1,
			importedAt: 'then',
		},
	},
});

describe('linkedUpdateStatuses', () => {
	it('flags a changed source as update available', () => {
		const result = linkedUpdateStatuses(
			libOf(1),
			scanOf(skill({ hash: 'h2' })),
		);
		expect(result).toEqual({ review: 'update-available' });
	});

	it('reports nothing for an unchanged source', () => {
		expect(linkedUpdateStatuses(libOf(1), scanOf(skill()))).toEqual({});
	});

	it('flags edited here and changed at source', () => {
		const result = linkedUpdateStatuses(
			libOf(2),
			scanOf(skill({ hash: 'h2' })),
		);
		expect(result).toEqual({ review: 'edited-and-changed' });
	});

	it('does not flag a skill only edited here', () => {
		expect(linkedUpdateStatuses(libOf(2), scanOf(skill()))).toEqual({});
	});

	it('marks a link whose source was not scanned, never deleted', () => {
		expect(linkedUpdateStatuses(libOf(1), scanOf())).toEqual({
			review: 'not-scanned',
		});
	});
});
