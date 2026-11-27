export type BoardFilterState = {
	selectedUser?: number | string | null;
	selectedCategory?: number | string | null;
	selectedPersona?: string | null;
	selectedPriorities?: string[] | null;
	searchText?: string | null;
};

export type UnpagedDropSave = 'orders' | 'move' | 'status';

export type DropPlanState = {
	sorted: boolean;
	subset: boolean;
	local: boolean;
};

/** Filters that leave only part of a column in memory (plugin filters only hide cards). */
export const isSubsetFiltered = (filter: BoardFilterState | null | undefined) =>
	!!(
		filter?.selectedUser ||
		filter?.selectedCategory ||
		filter?.selectedPersona ||
		filter?.searchText ||
		filter?.selectedPriorities?.length
	);

/**
 * How an unpaged column saves a drop: renumber the full column, place the card next to
 * its visible neighbour on the server, or change only the status when neither is safe.
 */
export const unpagedDropSave = (state: DropPlanState): UnpagedDropSave => {
	if (state.sorted) return 'status';
	if (!state.subset) return 'orders';
	return state.local ? 'status' : 'move';
};

export const canReorderCards = (state: DropPlanState) =>
	unpagedDropSave(state) !== 'status';
