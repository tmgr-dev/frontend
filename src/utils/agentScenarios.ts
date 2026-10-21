export interface RunScenario {
	id: string;
	label: string;
}

export interface FillScenario {
	label: string;
	template: string;
}

export const runScenarios: RunScenario[] = [
	{ id: 'morning_summary', label: 'Morning summary' },
	{ id: 'day_results', label: 'Day results' },
	{ id: 'week_results', label: 'Week results' },
	{ id: 'blockers_stale', label: 'Blockers and stale tasks' },
	{ id: 'estimate_similar', label: 'Estimate by similar tasks' },
];

export const fillScenarios: FillScenario[] = [
	{
		label: 'Break an epic into subtasks',
		template:
			'Break this epic into subtasks (propose only, do not create anything): ',
	},
	{
		label: 'Review a meeting',
		template:
			'Review this meeting: list action items with owners and draft tasks (propose only, do not create anything): ',
	},
];
