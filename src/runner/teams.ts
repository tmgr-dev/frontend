import type { RunnerSession } from './api';

export interface TeamRow {
	session: RunnerSession;
	label: string | null;
}

export const teamLabel = (session: RunnerSession): string | null => {
	const team = session.team;
	if (!team) return null;
	return team.role === 'lead' ? 'lead' : `w${team.worker ?? '?'}`;
};

const rank = (session: RunnerSession): number =>
	session.team?.role === 'lead' ? 0 : (session.team?.worker ?? Infinity);

export const groupTeams = (sessions: RunnerSession[]): TeamRow[] => {
	const members = new Map<string, RunnerSession[]>();
	for (const session of sessions) {
		const of = session.team?.of;
		if (of) members.set(of, [...(members.get(of) ?? []), session]);
	}
	const rows: TeamRow[] = [];
	const placed = new Set<string>();
	for (const session of sessions) {
		const of = session.team?.of;
		if (!of) {
			rows.push({ session, label: null });
			continue;
		}
		if (placed.has(of)) continue;
		placed.add(of);
		const ordered = [...(members.get(of) ?? [])].sort(
			(a, b) => rank(a) - rank(b),
		);
		for (const member of ordered)
			rows.push({ session: member, label: teamLabel(member) });
	}
	return rows;
};
