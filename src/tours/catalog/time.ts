import type { TourDef } from '../types';

const LIST = { name: 'WorkspaceTasksList', params: 'currentWorkspace' } as const;

export const timeTours: TourDef[] = [
  {
    id: 'time.timer',
    version: 1,
    since: '3.5.0',
    group: 'time',
    title: 'Run a timer',
    minutes: 1,
    triggers: [{ kind: 'first_run', route: 'human' }, { kind: 'hub' }],
    requires: {
      modules: ['task.countdown'],
      platforms: ['web', 'mobile_web', 'desktop', 'desktop_local'],
    },
    completeOn: 'timer.stopped',
    steps: [
      {
        id: 's1',
        type: 'wait',
        anchor: 'task-form.timer',
        waitFor: { event: 'timer.started' },
        takeMeRoute: LIST,
        title: 'Start the timer',
        body: "Press play. We'll move on as soon as it is running.",
      },
      {
        id: 's2',
        type: 'spot',
        anchor: 'nav.active-timers',
        optional: true,
        title: 'Running timers',
        body: 'Running timers stay visible here, even when the task is closed.',
      },
      {
        id: 's3',
        type: 'wait',
        anchor: 'task-form.timer',
        waitFor: { event: 'timer.stopped' },
        alreadyDone: 'has_timer_minute',
        takeMeRoute: LIST,
        title: 'Stop the timer',
        body: "Stop it when you're done. Time adds to the task.",
      },
      {
        id: 's4',
        type: 'tip',
        anchor: 'task-form.estimate',
        optional: true,
        title: 'Estimates',
        body: 'Set an estimate to see when you run over.',
      },
    ],
  },
];
