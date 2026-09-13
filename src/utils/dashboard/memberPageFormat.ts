import type { MemberTasksTab, TeamActivityWindow } from '@/types/dashboard';

const WINDOWS: TeamActivityWindow[] = ['today', '7d', '30d'];
const TABS: MemberTasksTab[] = ['touched', 'assigned', 'created', 'done'];

type QueryValue = string | string[] | null | undefined;

const single = (value: QueryValue): string =>
  (Array.isArray(value) ? value[0] : value) ?? '';

/** The window lives in the URL, so anything can arrive; the API rejects what it does not know. */
export function memberWindowFromQuery(value: QueryValue): TeamActivityWindow {
  const raw = single(value).trim().toLowerCase() as TeamActivityWindow;

  return WINDOWS.includes(raw) ? raw : '7d';
}

export function tasksTabFromQuery(value: QueryValue): MemberTasksTab {
  const raw = single(value).trim().toLowerCase() as MemberTasksTab;

  return TABS.includes(raw) ? raw : 'touched';
}

/** "25%" — or "<1%" for a share that is real but rounds to zero, null when there is no team total. */
export function percentOfTeam(seconds: number, teamSeconds: number): string | null {
  if (!Number.isFinite(seconds) || !Number.isFinite(teamSeconds) || teamSeconds <= 0) {
    return null;
  }

  const share = (seconds / teamSeconds) * 100;

  if (share > 0 && share < 1) {
    return '<1%';
  }

  return `${Math.round(share)}%`;
}

export function relativeAge(iso: string | null | undefined, now = Date.now()): string | null {
  if (!iso) {
    return null;
  }

  const at = Date.parse(iso);

  if (Number.isNaN(at)) {
    return null;
  }

  const seconds = Math.max(0, Math.floor((now - at) / 1000));

  if (seconds < 60) {
    return 'just now';
  }
  if (seconds < 3600) {
    return `${Math.floor(seconds / 60)}m ago`;
  }
  if (seconds < 86400) {
    return `${Math.floor(seconds / 3600)}h ago`;
  }

  return `${Math.floor(seconds / 86400)}d ago`;
}
