import { agentRunEvent } from '../agentRunEvents';

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

  it('emits nothing for an unknown finished run', () => {
    expect(agentRunEvent([], run(1, 'succeeded'), true)).toBeNull();
  });

  it('emits nothing for updates of a running run or stale versions', () => {
    expect(agentRunEvent([run(1, 'running')], run(1, 'running', 2), true)).toBeNull();
    expect(agentRunEvent([run(1, 'running', 3)], run(1, 'succeeded', 2), true)).toBeNull();
    expect(agentRunEvent([run(1, 'succeeded')], run(1, 'succeeded', 2), true)).toBeNull();
  });
});
