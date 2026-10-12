import { readFileSync } from 'fs';
import { join } from 'path';
import {
	DEFAULT_EXPORT_SETTINGS,
	toExportParams,
} from '../exportSettings';

const read = (file: string) =>
	readFileSync(join(__dirname, '../..', file), 'utf8');

const fresh = () => ({
	...DEFAULT_EXPORT_SETTINGS,
	weekend_days: [...DEFAULT_EXPORT_SETTINGS.weekend_days],
	custom_holidays: [...DEFAULT_EXPORT_SETTINGS.custom_holidays],
});

describe('export settings defaults match the Java export request defaults', () => {
	it('mirrors ExportRequestValidator defaults', () => {
		expect(DEFAULT_EXPORT_SETTINGS).toEqual({
			round_hours_up: false,
			zero_expected_weekends: false,
			weekend_days: [0, 6],
			custom_holidays: [],
			parse_date_from_title: true,
			week_start: 1,
		});
	});

	it('sends nothing for an untouched panel (same as the pre-panel {})', () => {
		expect(toExportParams(fresh())).toEqual({});
	});

	it('sends only what the user changed', () => {
		const s = fresh();
		s.round_hours_up = true;
		s.zero_expected_weekends = true;
		s.parse_date_from_title = false;
		s.weekend_days = [5, 6];
		s.custom_holidays = ['2026-10-12'];
		expect(toExportParams(s)).toEqual({
			round_hours_up: true,
			zero_expected_weekends: true,
			parse_date_from_title: false,
			weekend_days: [5, 6],
			custom_holidays: ['2026-10-12'],
		});
	});

	it('treats weekend days as a set and never sends week_start', () => {
		const s = fresh();
		s.weekend_days = [6, 0];
		s.week_start = 0;
		expect(toExportParams(s)).toEqual({});
	});

	it('never sends include_explanations', () => {
		const s: any = fresh();
		s.include_explanations = true;
		expect(toExportParams(s)).not.toHaveProperty('include_explanations');
	});
});

describe('export panel wiring', () => {
	it('has no AI explanations checkbox until the backend gates it', () => {
		const panel = read('components/tasks/ExportSettingsPanel.vue');
		expect(panel).not.toMatch(/include_explanations|AI explanations/);
	});

	it('TasksMultipleActionsModal has one computed block gating on module, cloud and known modules', () => {
		const source = read('components/tasks/TasksMultipleActionsModal.vue');
		expect(source.match(/^\t\tcomputed: \{/gm)).toHaveLength(1);
		expect(source).toMatch(/featureToggles\/modulesKnown/);
		expect(source).toMatch(/isLocalOnly/);
		expect(source).toMatch(/isFeatureEnabled'\]\(\s*'exports'/);
	});

	it('carries the export tour anchors', () => {
		const modal = read('components/tasks/TasksMultipleActionsModal.vue');
		expect(modal).toMatch(/data-tour="export\.panel"/);
		expect(modal).toMatch(/data-tour="export\.download"/);
		expect(read('components/tasks/TasksListComponent.vue')).toMatch(
			/data-tour="list\.process"/,
		);
	});
});
