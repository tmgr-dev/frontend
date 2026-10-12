import { agentRunEvent, showAgentWorkEmptyState } from '../agentRunEvents';

const run = (id: number, status: string, version = 1) => ({ id, status, version });

describe('agentRunEvent', () => {
  it('emits started for a new running run', () => {
    expect(agentRunEvent([], run(1, 'running'), true)).toBe('agent.run_started');
  });

  it('emits finished when a running run ends', () => {
    for (const status of ['succeeded', 'failed', 'cancelled']) {
      expect(agentRunEvent([run(1, 'running')], run(1, status, 2), true)).toBe(
        'agent.run_finished',
      );
    }
  });

  it('emits nothing before the initial load completed', () => {
    expect(agentRunEvent([], run(1, 'running'), false)).toBeNull();
  });

  it('emits started for an unknown run that is already finished', () => {
    expect(agentRunEvent([], run(1, 'succeeded'), true)).toBe('agent.run_started');
  });

  it('emits nothing for updates of a running run or stale versions', () => {
    expect(agentRunEvent([run(1, 'running')], run(1, 'running', 2), true)).toBeNull();
    expect(agentRunEvent([run(1, 'running', 3)], run(1, 'succeeded', 2), true)).toBeNull();
    expect(agentRunEvent([run(1, 'succeeded')], run(1, 'succeeded', 2), true)).toBeNull();
  });
});

describe('showAgentWorkEmptyState', () => {
  const anchor = 'task-form.agent-work';

  it('shows only for a loaded empty list while the agent-work tour step is active', () => {
    expect(showAgentWorkEmptyState({ loaded: true, runs: [], tourAnchor: anchor })).toBe(true);
  });

  it('stays hidden without a tour step on that anchor', () => {
    expect(showAgentWorkEmptyState({ loaded: true, runs: [], tourAnchor: null })).toBe(false);
    expect(showAgentWorkEmptyState({ loaded: true, runs: [], tourAnchor: 'task-form.assignees' })).toBe(false);
    expect(showAgentWorkEmptyState({ loaded: true, runs: [] })).toBe(false);
  });

  it('stays hidden before load or when runs exist', () => {
    expect(showAgentWorkEmptyState({ loaded: false, runs: [], tourAnchor: anchor })).toBe(false);
    expect(showAgentWorkEmptyState({ loaded: true, runs: [{}], tourAnchor: anchor })).toBe(false);
  });
});
