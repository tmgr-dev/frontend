import { isValidTimeZone } from '@/utils/timezone';

export interface TimeZoneDetectDeps {
	userId: number | undefined;
	loggedIn: boolean;
	saved: unknown;
	detected: string | null;
	save: (timezone: string) => Promise<string | undefined>;
	onSaved: (timezone: string) => void;
}

let attemptedFor: number | null | undefined = null;

export const resetTimeZoneDetection = () => {
	attemptedFor = null;
};

export const detectUserTimeZone = async (deps: TimeZoneDetectDeps) => {
	if (!deps.loggedIn || !deps.detected) return;
	if (isValidTimeZone(deps.saved)) return;
	if (attemptedFor === deps.userId) return;
	attemptedFor = deps.userId;
	try {
		const saved = await deps.save(deps.detected);
		if (saved) deps.onSaved(saved);
	} catch {}
};
