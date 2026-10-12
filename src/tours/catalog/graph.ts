import type { Platform, TourDef } from '../types';

const MAP = { name: 'WorkspaceMap', params: 'currentWorkspace' } as const;
const MODULES = { name: 'ModuleSettings' } as const;
const WDDL: Platform[] = ['web', 'desktop', 'desktop_local'];

export const graphTours: TourDef[] = [
  {
    id: 'graph.map',
    version: 1,
    since: '3.4.157',
    group: 'graph',
    title: 'Your workspace map',
    minutes: 1,
    triggers: [
      { kind: 'first_visit', routeName: 'WorkspaceMap' },
      { kind: 'feature_enabled', module: 'graph' },
      { kind: 'hub' },
    ],
    requires: { modules: ['graph', 'pages'], platforms: WDDL },
    steps: [
      {
        id: 's1',
        type: 'spot',
        anchor: 'map.canvas',
        route: MAP,
        title: 'The map',
        body: 'Tasks, pages and people as a map. Lines are links, relations and mentions.',
      },
      {
        id: 's2',
        type: 'spot',
        anchor: 'map.filters',
        title: 'Filters',
        body: 'Filter by type or category to see one corner of the workspace.',
      },
      {
        id: 's3',
        type: 'tip',
        anchor: 'map.search',
        title: 'Search',
        body: 'Search, or click any node to open it.',
      },
    ],
  },
  {
    id: 'graph.enable',
    version: 1,
    since: '3.4.157',
    group: 'graph',
    title: 'Turn on Graph',
    minutes: 1,
    triggers: [{ kind: 'hub' }],
    requires: { role: 'owner', modulesOff: ['graph'], platforms: WDDL },
    completeOn: 'module.enabled:graph',
    steps: [
      {
        id: 's1',
        type: 'wait',
        anchor: 'modules.graph',
        waitFor: { event: 'module.enabled:graph' },
        route: MODULES,
        title: 'Switch Graph on',
        body: 'Graph is off in this workspace. Switch it on to see the map.',
      },
    ],
  },
];
