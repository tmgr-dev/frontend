export interface RunScenario {
	id: string;
	label: string;
	labelKey: string;
}

export interface FillScenario {
	label: string;
	labelKey: string;
	template: string;
}

export const runScenarios: RunScenario[] = [
	{
		id: 'morning_summary',
		label: 'Morning summary',
		labelKey: 'agents.helpers.scenarios.run.morning_summary',
	},
	{
		id: 'day_results',
		label: 'Day results',
		labelKey: 'agents.helpers.scenarios.run.day_results',
	},
	{
		id: 'week_results',
		label: 'Week results',
		labelKey: 'agents.helpers.scenarios.run.week_results',
	},
	{
		id: 'blockers_stale',
		label: 'Blockers and stale tasks',
		labelKey: 'agents.helpers.scenarios.run.blockers_stale',
	},
	{
		id: 'estimate_similar',
		label: 'Estimate by similar tasks',
		labelKey: 'agents.helpers.scenarios.run.estimate_similar',
	},
];

export const fillScenarios: FillScenario[] = [
	{
		label: 'Break an epic into subtasks',
		labelKey: 'agents.helpers.scenarios.fill.epic',
		template:
			'Break this epic into subtasks (propose only, do not create anything): ',
	},
	{
		label: 'Review a meeting',
		labelKey: 'agents.helpers.scenarios.fill.meeting',
		template:
			'Review this meeting: list action items with owners and draft tasks (propose only, do not create anything): ',
	},
];
