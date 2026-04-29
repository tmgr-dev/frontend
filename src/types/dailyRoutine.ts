export type RoutineCategoryId =
	| 'work'
	| 'health'
	| 'learn'
	| 'home'
	| 'social'
	| 'none';

export interface RoutineCategory {
	id: RoutineCategoryId;
	name: string;
	color: string;
}

export type RoutineFrequency = 'NONE' | 'DAILY' | 'WEEKLY' | 'MONTHLY' | 'YEARLY';

export interface RoutinePattern {
	frequency: RoutineFrequency;
	interval: number;
	days_of_week?: string[] | null;
	day_of_frequency?: number | null;
	month?: number | null;
	occurrences?: number | null;
	start_at?: string | null;
	end_at?: string | null;
	scheduled_time?: { hours: number; minutes: number; seconds: number } | string | null;
	duration_min?: number | null;
	reminder_min?: number | null;
}

export interface RoutineEntry {
	task_id: number;
	title: string;
	description: string | null;
	routine_category: RoutineCategory;
	date: string; // YYYY-MM-DD
	time: string | null; // HH:mm
	duration_min: number | null;
	reminder_min: number | null;
	completed: boolean;
	status: 'PENDING' | 'COMPLETED' | 'SKIPPED' | null;
	instance_id: number | null;
	virtual: boolean;
	frequency: RoutineFrequency;
	scheduled_for: string | null;
}

export interface YearStats {
	[date: string]: { fires: number; completed: number };
}

export interface IcsImportResult {
	created: number;
	skipped: number;
	replaced: number;
	errors: string[];
}

export type ViewId = 'list' | 'day' | 'week' | 'month' | 'year';
