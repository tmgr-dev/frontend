import { ONBOARDING_KEY } from '@/local/onboarding';
import { ref } from 'vue';

export type GhostStage = 'watch' | 'wheel' | 'finished';

const STAGES: GhostStage[] = ['watch', 'wheel', 'finished'];

interface StageStorage {
	getItem(key: string): string | null;
	setItem(key: string, value: string): void;
}

const parse = (raw: string | null): Record<string, unknown> => {
	try {
		const value = raw ? JSON.parse(raw) : null;
		if (value && typeof value === 'object' && !Array.isArray(value))
			return value;
	} catch {
		/* unreadable state counts as none */
	}
	return {};
};

export const readGhostStage = (storage: StageStorage): GhostStage | null => {
	try {
		const stage = parse(storage.getItem(ONBOARDING_KEY)).ghost_stage;
		return STAGES.includes(stage as GhostStage) ? (stage as GhostStage) : null;
	} catch {
		return null;
	}
};

export const writeGhostStage = (
	storage: StageStorage,
	stage: GhostStage,
): void => {
	try {
		const state = parse(storage.getItem(ONBOARDING_KEY));
		storage.setItem(
			ONBOARDING_KEY,
			JSON.stringify({ ...state, ghost_stage: stage }),
		);
	} catch {
		/* storage unavailable: the entry flow just does not resume */
	}
};

const safeStorage = (): StageStorage | null => {
	try {
		return typeof localStorage === 'undefined' ? null : localStorage;
	} catch {
		return null;
	}
};

const initial = (): GhostStage | null => {
	const storage = safeStorage();
	return storage ? readGhostStage(storage) : null;
};

export const ghostStage = ref<GhostStage | null>(initial());

export const setGhostStage = (stage: GhostStage): void => {
	const storage = safeStorage();
	if (storage) writeGhostStage(storage, stage);
	ghostStage.value = stage;
};

export const advanceToWheel = (): void => {
	if (ghostStage.value === 'watch') setGhostStage('wheel');
};
