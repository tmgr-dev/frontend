import type { TourDef } from '../types';

const LIST = { name: 'WorkspaceTasksList', params: 'currentWorkspace' } as const;
const PERSONAS = { name: 'PersonaSettings' } as const;

export const personasTours: TourDef[] = [
  {
    id: 'personas.create',
    version: 1,
    since: '3.4.156',
    group: 'personas',
    title: 'Give your agent an identity',
    minutes: 2,
    triggers: [{ kind: 'first_visit', routeName: 'PersonaSettings' }, { kind: 'hub' }],
    requires: { modules: ['personas'], account: 'cloud', platforms: ['web', 'desktop'] },
    steps: [
      {
        id: 's1',
        type: 'wait',
        anchor: 'personas.create',
        waitFor: { event: 'persona.created' },
        route: PERSONAS,
        title: 'Create a persona',
        body: 'Create a persona: a name, a system prompt and skills for one agent.',
      },
      {
        id: 's2',
        type: 'spot',
        anchor: 'personas.grants',
        title: 'Workspace access',
        body: 'Grant it workspaces and permissions: allowed, read only or forbidden.',
      },
    ],
  },
  {
    id: 'personas.token',
    version: 1,
    since: '3.4.156',
    group: 'personas',
    title: 'Persona token',
    minutes: 1,
    triggers: [{ kind: 'hub' }],
    requires: { modules: ['personas'], account: 'cloud', platforms: ['web', 'desktop'] },
    steps: [
      {
        id: 's1',
        type: 'wait',
        anchor: 'personas.tokens',
        waitFor: { event: 'persona.token_created' },
        route: PERSONAS,
        title: 'Create a token',
        body: 'Create a token for this persona. Valid 1 to 365 days, shown once.',
      },
    ],
  },
  {
    id: 'personas.assign',
    version: 1,
    since: '3.4.156',
    group: 'personas',
    title: 'Assign a task to a persona',
    minutes: 1,
    triggers: [{ kind: 'hub' }],
    requires: { modules: ['personas', 'task.assignees'], account: 'cloud', platforms: ['web', 'desktop'] },
    steps: [
      {
        id: 's1',
        type: 'wait',
        anchor: 'task-form.assignees',
        waitFor: { event: 'persona.assigned' },
        takeMeRoute: LIST,
        title: 'Assign the task',
        body: "Assign this task to your persona. It shows up in the agent's queue.",
      },
      {
        id: 's2',
        type: 'tip',
        anchor: 'task-form.persona-assignees',
        optional: true,
        title: 'The agent picks it up',
        body: 'The agent reads its queue with list_my_queue and picks the task up.',
      },
    ],
  },
  {
    id: 'personas.skills',
    version: 1,
    since: '3.4.156',
    group: 'personas',
    title: 'Persona skills',
    minutes: 1,
    triggers: [{ kind: 'hub' }],
    requires: { modules: ['personas'], account: 'cloud', platforms: ['web', 'desktop'] },
    steps: [
      {
        id: 's1',
        type: 'spot',
        anchor: 'personas.skills',
        route: PERSONAS,
        title: 'Skills',
        body: 'Skills are reusable instructions the agent loads when it needs them.',
      },
    ],
  },
];
