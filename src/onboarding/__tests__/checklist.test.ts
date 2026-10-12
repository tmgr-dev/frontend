import { buildChecklist } from '../checklist';
import type { ChecklistDeps } from '../checklist';

const deps = (over: Partial<ChecklistDeps> = {}): ChecklistDeps => ({
  isModuleEnabled: () => true,
  modulesKnown: () => true,
  canManageModules: true,
  account: 'cloud',
  emailVerified: true,
  ...over,
});

const ids = (c: ReturnType<typeof buildChecklist>) => c.items.map((i) => i.id);

describe('buildChecklist', () => {
  it('builds the human list and counts done items', () => {
    const list = buildChecklist('human', { has_task: true, has_page: 1 }, deps());
    expect(list.items.map((i) => i.label)).toEqual([
      'Create a task',
      'Run a timer for a minute',
      'Move a card on the board',
      'Add a daily routine',
      'Write a page',
      'Connect an AI agent',
    ]);
    expect(list.items.filter((i) => i.done).map((i) => i.label)).toEqual([
      'Create a task',
      'Write a page',
    ]);
    expect(list.items[5].optional).toBe(true);
    expect(list.done).toBe(2);
    expect(list.total).toBe(6);
  });

  it('builds the agent list', () => {
    const list = buildChecklist('agent', { mcp_first_call_at: '2026-10-12T10:00:00Z' }, deps());
    expect(list.items.map((i) => i.label)).toEqual([
      'Connect an agent',
      'Run your first agent task',
      'Get a phone alert when your agent needs you',
      'Start a timer on a task',
      'Write a context page your agents read',
      'Invite a teammate',
    ]);
    expect(list.done).toBe(1);
  });

  it.each([null, 'skipped'])('intent %p gets the human list', (intent) => {
    expect(ids(buildChecklist(intent, {}, deps()))).toEqual(ids(buildChecklist('human', {}, deps())));
  });

  it('never lists the graph', () => {
    for (const intent of ['human', 'agent']) {
      expect(buildChecklist(intent, {}, deps()).items.map((i) => i.label.toLowerCase()).join()).not.toMatch(
        /graph/,
      );
    }
  });

  it('greys an item whose module is off, with the owner message', () => {
    const list = buildChecklist(
      'human',
      {},
      deps({ isModuleEnabled: (key) => key !== 'daily_routines' }),
    );
    const item = list.items.find((i) => i.label === 'Add a daily routine')!;
    expect(item.disabledMessage).toBe('Turn on Daily routines in Settings -> Modules');
    expect(list.items.filter((i) => i.disabledMessage)).toHaveLength(1);
  });

  it('tells a member to ask the owner', () => {
    const list = buildChecklist(
      'human',
      {},
      deps({ isModuleEnabled: (key) => key !== 'board', canManageModules: false }),
    );
    expect(list.items.find((i) => i.id === 'move-card')!.disabledMessage).toBe(
      'Ask the workspace owner to turn on Board',
    );
  });

  it('does not grey anything until modules are known', () => {
    const list = buildChecklist(
      'human',
      {},
      deps({ isModuleEnabled: () => false, modulesKnown: () => false }),
    );
    expect(list.items.some((i) => i.disabledMessage)).toBe(false);
  });

  it('a done item stays done when its module is off', () => {
    const list = buildChecklist('human', { has_page: true }, deps({ isModuleEnabled: () => false }));
    expect(list.items.find((i) => i.id === 'write-page')!.done).toBe(true);
  });

  it('gates the invite item on email verification', () => {
    const blocked = buildChecklist('agent', {}, deps({ emailVerified: false }));
    expect(blocked.items.find((i) => i.id === 'invite-teammate')!.disabledMessage).toBe(
      'Verify your email first',
    );
    const open = buildChecklist('agent', {}, deps({ emailVerified: true }));
    expect(open.items.find((i) => i.id === 'invite-teammate')!.disabledMessage).toBeUndefined();
  });

  it('reads email_verified from facts when the dep is absent', () => {
    const list = buildChecklist('agent', { email_verified: true }, deps({ emailVerified: undefined }));
    expect(list.items.find((i) => i.id === 'invite-teammate')!.disabledMessage).toBeUndefined();
  });

  it('uses mcp, agent_work and alerts module gates for the agent list', () => {
    const list = buildChecklist(
      'agent',
      {},
      deps({ isModuleEnabled: (key) => !['mcp', 'agent_work', 'alerts'].includes(key) }),
    );
    expect(list.items.filter((i) => i.disabledMessage).map((i) => i.id)).toEqual([
      'connect-agent',
      'first-agent-run',
      'phone-alert',
    ]);
  });

  it('drops cloud-only items for a local account', () => {
    const list = buildChecklist('human', {}, deps({ account: 'local' }));
    expect(ids(list)).not.toContain('connect-agent');
    expect(list.total).toBe(5);
  });
});
