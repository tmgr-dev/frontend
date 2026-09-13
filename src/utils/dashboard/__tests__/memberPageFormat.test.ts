import {
  memberWindowFromQuery,
  percentOfTeam,
  relativeAge,
  tasksTabFromQuery,
} from '@/utils/dashboard/memberPageFormat';

describe('memberWindowFromQuery', () => {
  it('defaults to a week and accepts the three windows', () => {
    expect(memberWindowFromQuery(undefined)).toBe('7d');
    expect(memberWindowFromQuery('')).toBe('7d');
    expect(memberWindowFromQuery('today')).toBe('today');
    expect(memberWindowFromQuery('30d')).toBe('30d');
  });

  it('falls back to a week rather than sending garbage to the API', () => {
    expect(memberWindowFromQuery('last-year')).toBe('7d');
    expect(memberWindowFromQuery(['7d', '30d'])).toBe('7d');
  });
});

describe('tasksTabFromQuery', () => {
  it('defaults to what the member touched last', () => {
    expect(tasksTabFromQuery(undefined)).toBe('touched');
    expect(tasksTabFromQuery('nonsense')).toBe('touched');
  });

  it('accepts every tab the backend knows', () => {
    expect(tasksTabFromQuery('assigned')).toBe('assigned');
    expect(tasksTabFromQuery('created')).toBe('created');
    expect(tasksTabFromQuery('done')).toBe('done');
  });
});

describe('percentOfTeam', () => {
  it('reads as a share of the team total', () => {
    expect(percentOfTeam(3600, 14400)).toBe('25%');
    expect(percentOfTeam(1, 1)).toBe('100%');
  });

  it('says nothing when there is no team total to compare against', () => {
    expect(percentOfTeam(3600, 0)).toBeNull();
    expect(percentOfTeam(0, 0)).toBeNull();
  });

  it('never rounds a real contribution down to nothing', () => {
    expect(percentOfTeam(1, 100000)).toBe('<1%');
  });
});

describe('relativeAge', () => {
  const now = Date.parse('2026-09-13T12:00:00Z');

  it('counts in minutes, hours and days', () => {
    expect(relativeAge('2026-09-13T11:59:30Z', now)).toBe('just now');
    expect(relativeAge('2026-09-13T11:30:00Z', now)).toBe('30m ago');
    expect(relativeAge('2026-09-13T09:00:00Z', now)).toBe('3h ago');
    expect(relativeAge('2026-09-10T12:00:00Z', now)).toBe('3d ago');
  });

  it('has nothing to say about a member who never touched anything', () => {
    expect(relativeAge(null, now)).toBeNull();
    expect(relativeAge('not-a-date', now)).toBeNull();
  });
});
