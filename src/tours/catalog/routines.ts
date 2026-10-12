import type { Platform, TourDef } from '../types';

const ROUTINES = { name: 'DailyRoutines' } as const;
const ALL: Platform[] = ['web', 'mobile_web', 'desktop', 'desktop_local'];
const WDDL: Platform[] = ['web', 'desktop', 'desktop_local'];

export const routinesTours: TourDef[] = [
  {
    id: 'routines.first',
    version: 1,
    since: '3.4.157',
    group: 'routines',
    title: 'Add a daily routine',
    minutes: 1,
    triggers: [{ kind: 'first_visit', routeName: 'DailyRoutines' }, { kind: 'hub' }],
    requires: { modules: ['daily_routines'], platforms: ALL },
    steps: [
      {
        id: 's1',
        type: 'wait',
        anchor: 'routines.add',
        mobileAnchor: 'routines.quick-add',
        waitFor: { event: 'routine.created' },
        alreadyDone: 'has_routine',
        takeMeRoute: ROUTINES,
        title: 'Add a routine',
        body: 'Add something you repeat: daily, weekly, monthly or yearly.',
      },
      {
        id: 's2',
        type: 'wait',
        anchor: 'routines.instance',
        waitFor: { event: 'routine.completed' },
        optional: true,
        title: 'Tick it off',
        body: 'Tick it off for today.',
      },
    ],
  },
  {
    id: 'routines.views',
    version: 1,
    since: '3.4.157',
    group: 'routines',
    title: 'Day, week, month, year',
    minutes: 1,
    triggers: [{ kind: 'hub' }],
    requires: { platforms: ALL },
    steps: [
      {
        id: 's1',
        type: 'spot',
        anchor: 'routines.view-switcher',
        route: ROUTINES,
        title: 'Views',
        body: 'Switch views. The year view is a heatmap of what you kept up.',
      },
    ],
  },
  {
    id: 'routines.reminders',
    version: 1,
    since: '3.4.157',
    group: 'routines',
    title: 'Reminders and conversion',
    minutes: 1,
    triggers: [{ kind: 'hub' }],
    requires: { platforms: WDDL },
    steps: [
      {
        id: 's1',
        type: 'wait',
        anchor: 'routines.instance',
        waitFor: { event: 'routine.editor_opened' },
        takeMeRoute: ROUTINES,
        title: 'Open a routine',
        body: 'Open a routine to edit it. Add one first if the list is empty.',
      },
      {
        id: 's2',
        type: 'spot',
        anchor: 'routine.reminder',
        title: 'Reminder',
        body: 'Set a reminder before a routine is due.',
      },
      {
        id: 's3',
        type: 'spot',
        anchor: 'routine.to-task',
        optional: true,
        title: 'Convert to a task',
        body: 'Turn a routine into a task when it grows.',
      },
    ],
  },
];
