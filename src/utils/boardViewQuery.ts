import type { BoardFilterState } from '@/types/store';
import { MY_PERSONAS_FILTER } from '@/utils/boardLoading';
import { serializePriorityFilter } from '@/utils/priority';
import type { LocationQuery } from 'vue-router';

const filterKeys = [
	'search',
	'user',
	'persona',
	'my_personas',
	'category',
	'priority',
	'priority_sort',
] as const;
export type BoardViewQuery = Partial<
	Record<(typeof filterKeys)[number], string>
>;

export function boardViewQuery(filter: BoardFilterState): BoardViewQuery {
	const query: BoardViewQuery = {};
	if (filter.searchText) query.search = filter.searchText;
	if (filter.selectedUser) query.user = String(filter.selectedUser);
	if (filter.selectedCategory) query.category = String(filter.selectedCategory);
	if (filter.selectedPersona === MY_PERSONAS_FILTER) query.my_personas = '1';
	else if (filter.selectedPersona) query.persona = filter.selectedPersona;
	const priority = serializePriorityFilter(filter.selectedPriorities);
	if (priority) query.priority = priority;
	if (filter.prioritySort) query.priority_sort = '1';
	return query;
}

export function withBoardView(
	current: LocationQuery,
	view: BoardViewQuery,
): LocationQuery {
	const query = { ...current };
	for (const key of filterKeys) {
		if (typeof view[key] === 'string') query[key] = view[key];
		else delete query[key];
	}
	return query;
}
