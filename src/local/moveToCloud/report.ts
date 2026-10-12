import type { ImportError, ImportReport, ImportStatus } from './api';

const ENTITY_LABELS: Record<string, string> = {
	personas: 'Personas',
	statuses: 'Statuses',
	categories: 'Categories',
	tasks: 'Tasks',
	comments: 'Comments',
	files: 'Files',
	pages: 'Pages',
	page_versions: 'Page versions',
	routines: 'Routines',
	agent_runs: 'Agent runs',
	assignments: 'Assignments',
	relations: 'Relations',
	modules: 'Modules',
};

const SKIP_REASONS: Record<string, string> = {
	excluded: 'left out of the move',
	not_uploaded: 'not uploaded',
	not_allowed: 'not allowed',
	persona_is_me: 'belongs to you',
	disabled: 'disabled',
	unsupported: 'not supported',
	duplicate: 'duplicate',
	unknown_module: 'unknown module',
	unmapped_target: 'its target was not moved',
};

const label = (entity: string) =>
	ENTITY_LABELS[entity] ??
	entity.charAt(0).toUpperCase() + entity.slice(1).replace(/_/g, ' ');

export const skipReasonLabel = (reason: string) =>
	SKIP_REASONS[reason] ?? reason.replace(/_/g, ' ');

export const reportLines = (report: ImportReport | null | undefined) => {
	if (!report) return [];
	const lines: string[] = [];
	for (const [entity, c] of Object.entries(report.counts ?? {})) {
		if (c.skipped > 0 || c.imported !== c.expected) {
			lines.push(
				`${label(entity)}: ${c.imported} of ${c.expected} moved` +
					(c.skipped > 0 ? `, ${c.skipped} skipped` : ''),
			);
		}
	}
	for (const file of report.skipped_files ?? []) {
		lines.push(`${file.name}: skipped, ${skipReasonLabel(file.reason)}`);
	}
	if (report.unmapped_refs > 0) {
		lines.push(
			`${report.unmapped_refs} ${
				report.unmapped_refs === 1 ? 'reference points' : 'references point'
			} to something that was not moved`,
		);
	}
	return lines;
};

const errorLine = (error: ImportError): string => {
	switch (error.code) {
		case 'count_mismatch':
			return `${label(error.entity ?? '')}: expected ${error.expected}, moved ${
				error.imported
			}, skipped ${error.skipped}`;
		case 'bundle_count_mismatch':
			return `${label(error.entity ?? '')}: the bundle held ${
				error.actual
			} of ${error.expected} items`;
		case 'unmapped_refs':
			return `${error.count} references point to something that was not moved`;
		default:
			return error.message ?? error.code ?? 'The move failed on the server.';
	}
};

export const serverErrorLines = (
	status: Pick<ImportStatus, 'errors'> | null | undefined,
): string[] => (status?.errors ?? []).map((e) => errorLine(e as ImportError));
