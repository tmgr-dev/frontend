import { reportLines, serverErrorLines } from '../report';

describe('report', () => {
	it('lists differences, skipped files and unmapped references', () => {
		const lines = reportLines({
			counts: {
				tasks: { expected: 10, imported: 10, skipped: 0 },
				comments: { expected: 5, imported: 3, skipped: 2 },
				page_versions: { expected: 4, imported: 4, skipped: 0 },
			},
			skipped_files: [
				{ local_id: '7', name: 'a.exe', reason: 'not_allowed' },
				{ local_id: '8', name: 'b.png', reason: 'not_uploaded' },
			],
			unmapped_refs: 2,
		});
		expect(lines).toEqual([
			'Comments: 3 of 5 moved, 2 skipped',
			'a.exe: skipped, not allowed',
			'b.png: skipped, not uploaded',
			'2 references point to something that was not moved',
		]);
	});

	it('returns nothing without a report', () => {
		expect(reportLines(null)).toEqual([]);
	});

	it('describes server errors', () => {
		expect(
			serverErrorLines({
				errors: [
					{
						phase: 'verify',
						code: 'count_mismatch',
						entity: 'tasks',
						expected: 3,
						imported: 2,
						skipped: 0,
					},
					{ phase: 'verify', code: 'unmapped_refs', count: 4 },
					{ phase: 'files', code: 'phase_failed', message: 'Copy failed' },
				],
			}),
		).toEqual([
			'Tasks: expected 3, moved 2, skipped 0',
			'4 references point to something that was not moved',
			'Copy failed',
		]);
	});
});
