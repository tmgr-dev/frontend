import { t } from '@/i18n';
import type { ImportError, ImportReport, ImportStatus } from './api';

const ENTITY_KEYS = new Set([
	'personas',
	'statuses',
	'categories',
	'tasks',
	'comments',
	'files',
	'pages',
	'page_versions',
	'routines',
	'agent_runs',
	'assignments',
	'relations',
	'modules',
]);

const SKIP_REASONS = new Set([
	'excluded',
	'not_uploaded',
	'not_allowed',
	'persona_is_me',
	'disabled',
	'unsupported',
	'duplicate',
	'unknown_module',
	'unmapped_target',
]);

const label = (entity: string) =>
	ENTITY_KEYS.has(entity)
		? t(`desktop.helpers.move.entities.${entity}`)
		: entity.charAt(0).toUpperCase() + entity.slice(1).replace(/_/g, ' ');

export const skipReasonLabel = (reason: string) =>
	SKIP_REASONS.has(reason)
		? t(`desktop.helpers.move.skipReasons.${reason}`)
		: reason.replace(/_/g, ' ');

export const reportLines = (report: ImportReport | null | undefined) => {
	if (!report) return [];
	const lines: string[] = [];
	for (const [entity, c] of Object.entries(report.counts ?? {})) {
		if (c.skipped > 0 || c.imported !== c.expected) {
			lines.push(
				t(
					c.skipped > 0
						? 'desktop.helpers.move.report.movedSkipped'
						: 'desktop.helpers.move.report.moved',
					{
						entity: label(entity),
						imported: c.imported,
						expected: c.expected,
						skipped: c.skipped,
					},
				),
			);
		}
	}
	for (const file of report.skipped_files ?? []) {
		lines.push(
			t('desktop.helpers.move.report.fileSkipped', {
				name: file.name,
				reason: skipReasonLabel(file.reason),
			}),
		);
	}
	if (report.unmapped_refs > 0) {
		lines.push(t('desktop.helpers.move.report.unmapped', report.unmapped_refs));
	}
	return lines;
};

const errorLine = (error: ImportError): string => {
	switch (error.code) {
		case 'count_mismatch':
			return t('desktop.helpers.move.report.countMismatch', {
				entity: label(error.entity ?? ''),
				expected: error.expected,
				imported: error.imported,
				skipped: error.skipped,
			});
		case 'bundle_count_mismatch':
			return t('desktop.helpers.move.report.bundleMismatch', {
				entity: label(error.entity ?? ''),
				actual: error.actual,
				expected: error.expected,
			});
		case 'unmapped_refs':
			return t('desktop.helpers.move.report.unmappedRefs', {
				count: error.count,
			});
		default:
			return (
				error.message ?? error.code ?? t('desktop.helpers.move.report.failed')
			);
	}
};

export const serverErrorLines = (
	status: Pick<ImportStatus, 'errors'> | null | undefined,
): string[] => (status?.errors ?? []).map((e) => errorLine(e as ImportError));
