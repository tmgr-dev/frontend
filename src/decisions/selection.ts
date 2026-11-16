import type { CardOption } from './types';

export interface SelectionState {
	selected: string | null;
	noteFor: string | null;
}

export interface Reconciled extends SelectionState {
	cleared: boolean;
}

const same = (a: CardOption | undefined, b: CardOption | undefined): boolean =>
	Boolean(a && b && a.id === b.id && a.label === b.label);

export const reconcileSelection = (
	previous: CardOption[],
	next: CardOption[],
	state: SelectionState,
): Reconciled => {
	const survives = (id: string | null): boolean =>
		id === null ||
		same(
			previous.find((o) => o.id === id),
			next.find((o) => o.id === id),
		);
	const keepSelected = survives(state.selected);
	const keepNote = survives(state.noteFor);
	if (keepSelected && keepNote) return { ...state, cleared: false };
	const recommended = next.find((o) => o.recommended)?.id ?? null;
	return {
		selected: keepSelected ? state.selected : recommended,
		noteFor: keepNote ? state.noteFor : null,
		cleared: true,
	};
};
