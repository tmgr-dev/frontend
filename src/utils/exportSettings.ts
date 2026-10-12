export interface ExportPanelSettings {
	round_hours_up: boolean;
	zero_expected_weekends: boolean;
	weekend_days: number[];
	custom_holidays: string[];
	parse_date_from_title: boolean;
}

export const DEFAULT_EXPORT_SETTINGS: ExportPanelSettings = {
	round_hours_up: false,
	zero_expected_weekends: false,
	weekend_days: [0, 6],
	custom_holidays: [],
	parse_date_from_title: true,
};

const sameSet = (a: number[], b: number[]) =>
	a.length === b.length && a.every((day) => b.includes(day));

export function toExportParams(
	settings: ExportPanelSettings,
): Record<string, boolean | number[] | string[]> {
	const params: Record<string, boolean | number[] | string[]> = {};
	if (settings.round_hours_up) params.round_hours_up = true;
	if (settings.zero_expected_weekends) params.zero_expected_weekends = true;
	if (!settings.parse_date_from_title) params.parse_date_from_title = false;
	if (!sameSet(settings.weekend_days, DEFAULT_EXPORT_SETTINGS.weekend_days))
		params.weekend_days = [...settings.weekend_days];
	if (settings.custom_holidays.length)
		params.custom_holidays = [...settings.custom_holidays];
	return params;
}
