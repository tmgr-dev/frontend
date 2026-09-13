import type {
	MemberTaskRow,
	MemberTasksPage,
	MemberTasksTab,
	TeamActivityWindow,
} from '@/types/dashboard';

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

/**
 * The tasks endpoint answers `{ data, total, page, per_page, tab, window }` and the API does not
 * put that behind another `data` key the way it does for a plain object; accept both anyway, and
 * never hand the caller an undefined list.
 */
export function memberTasksPage(body: unknown): MemberTasksPage {
	const raw = (body ?? {}) as Record<string, unknown>;
	const nested = raw.data as Record<string, unknown> | undefined;
	const page = (Array.isArray(raw.data) || raw.total !== undefined
		? raw
		: (nested ?? {})) as Record<string, unknown>;

	return {
		data: (Array.isArray(page.data) ? page.data : []) as MemberTaskRow[],
		total: typeof page.total === 'number' ? page.total : 0,
		page: typeof page.page === 'number' ? page.page : 1,
		per_page: typeof page.per_page === 'number' ? page.per_page : 20,
		tab: (page.tab as MemberTasksTab) ?? 'touched',
		window: (page.window as TeamActivityWindow) ?? '7d',
	};
}
