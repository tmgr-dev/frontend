import type { AxiosInstance } from 'axios';
import { normalizePath } from '@/local/router';

export const KNOWN_EVENTS = [
  'task.created',
  'task.status_changed',
  'timer.started',
  'timer.stopped',
  'comment.created',
  'relation.created',
  'category.created',
  'status.created',
  'page.created',
  'persona.created',
  'persona.token_created',
  'persona.assigned',
  'notify.token_created',
  'mcp.token_created',
  'invite.sent',
  'pomodoro.started',
  'routine.created',
  'ai.question_sent',
  'export.downloaded',
  'backup.created',
  'task.create_opened',
  'ai.panel_opened',
  'connect.command_copied',
  'tasks.selected',
  'page.link_added',
  'agent.first_mcp_call',
  'agent.run_started',
  'agent.run_finished',
  'agent.first_local_call',
  'routine.completed',
  'routine.editor_opened',
  'task.created_from_page',
  'checkpoint.added',
  'page.context_saved',
  'tasks.actions_opened',
] as const;

export type KnownEvent = (typeof KNOWN_EVENTS)[number];
export type ModuleEnabledEvent = `module.enabled:${string}`;
export type TourEventNameKey = KnownEvent | ModuleEnabledEvent;

export type TourEventPayload = Record<string, string | number | boolean | null>;

export interface TourEvent {
  name: TourEventNameKey;
  payload: TourEventPayload;
}

export type TourEventHandler = (
  payload: TourEventPayload,
  name: TourEventNameKey,
) => void;

export interface TourBus {
  emit(name: TourEventNameKey, payload?: TourEventPayload): void;
  on(name: TourEventNameKey, fn: TourEventHandler): () => void;
}

export function createTourBus(): TourBus {
  const handlers = new Map<string, Set<TourEventHandler>>();
  return {
    emit(name, payload = {}) {
      for (const fn of [...(handlers.get(name) ?? [])]) {
        try {
          fn(payload, name);
        } catch (error) {
          console.error('tour event handler failed', error);
        }
      }
    },
    on(name, fn) {
      const set = handlers.get(name) ?? new Set();
      set.add(fn);
      handlers.set(name, set);
      return () => {
        set.delete(fn);
      };
    },
  };
}

export const tourBus: TourBus = createTourBus();

export type ResolveStatusType = (statusId: number) => string | undefined;

export interface MatchContext {
  statusType?: ResolveStatusType;
  knownStatus?: (taskId: number) => number | undefined;
}

const isRecord = (value: unknown): value is Record<string, any> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const parseBody = (body: unknown): Record<string, any> => {
  if (typeof body === 'string') {
    try {
      const parsed = JSON.parse(body);
      return isRecord(parsed) ? parsed : {};
    } catch {
      return {};
    }
  }
  return isRecord(body) ? body : {};
};

const entityId = (responseData: unknown): number | null => {
  if (!isRecord(responseData)) return null;
  const entity = isRecord(responseData.data) ? responseData.data : responseData;
  const id = Number(entity.id);
  return Number.isFinite(id) && entity.id != null ? id : null;
};

export function matchRequest(
  method: string,
  url: string,
  body: unknown,
  responseData: unknown,
  ctx: MatchContext = {},
): TourEvent[] {
  const verb = method.toUpperCase();
  const path = normalizePath(url);
  const parsed = parseBody(body);
  const event = (name: TourEventNameKey, payload: TourEventPayload = {}): TourEvent => ({
    name,
    payload,
  });
  const statusEvent = (taskId: number | null, statusId: number): TourEvent =>
    event('task.status_changed', {
      taskId,
      statusId,
      statusType: ctx.statusType?.(statusId) ?? null,
    });
  let m: RegExpMatchArray | null;

  if (verb === 'POST') {
    if (path === 'tasks') return [event('task.created', { taskId: entityId(responseData) })];
    if ((m = path.match(/^tasks\/(\d+)\/countdown$/)))
      return [event('timer.started', { taskId: Number(m[1]) })];
    if ((m = path.match(/^tasks\/(\d+)\/comments$/)))
      return [event('comment.created', { taskId: Number(m[1]) })];
    if ((m = path.match(/^workspaces\/(\d+)\/statuses$/)))
      return [event('status.created', { workspaceId: Number(m[1]) })];
    if (path === 'project_categories') return [event('category.created')];
    if (path === 'pages') return [event('page.created')];
    if (path === 'personas') return [event('persona.created')];
    if (/^personas\/[^/]+\/tokens$/.test(path)) return [event('persona.token_created')];
    if ((m = path.match(/^tasks\/(\d+)\/personas\/[^/]+$/)))
      return [event('persona.assigned', { taskId: Number(m[1]) })];
    if (path === 'notify-tokens') return [event('notify.token_created')];
    if (path === 'smart-device/token/generate') return [event('mcp.token_created')];
    if (/^workspaces\/\d+\/invitations$/.test(path)) return [event('invite.sent')];
    if ((m = path.match(/^tasks\/(\d+)\/pomodoro$/)))
      return [event('pomodoro.started', { taskId: Number(m[1]) })];
    if ((m = path.match(/^tasks\/(\d+)\/related-to\/\d+\/with\/[^/]+$/)))
      return [event('relation.created', { taskId: Number(m[1]) })];
    if (/^agent\/conversations\/[^/]+\/messages$/.test(path)) return [event('ai.question_sent')];
    return [];
  }

  if (verb === 'DELETE') {
    if ((m = path.match(/^tasks\/(\d+)\/countdown$/)))
      return [event('timer.stopped', { taskId: Number(m[1]) })];
    return [];
  }

  if (verb === 'GET') {
    return /^exports\/tasks\/[^/]+$/.test(path) ? [event('export.downloaded')] : [];
  }

  if (verb === 'PUT' || verb === 'PATCH') {
    if ((m = path.match(/^tasks\/(\d+)\/(\d+)$/)) && verb === 'PUT')
      return [statusEvent(Number(m[1]), Number(m[2]))];
    if ((m = path.match(/^statuses\/(\d+)\/tasks$/)) && verb === 'PUT') {
      const ids = Array.isArray(parsed.task_ids) ? parsed.task_ids : [];
      return ids.map((id: unknown) => statusEvent(Number(id), Number(m![1])));
    }
    if ((m = path.match(/^tasks\/(\d+)$/)) && parsed.status_id != null) {
      const taskId = Number(m[1]);
      const next = Number(parsed.status_id);
      const known = ctx.knownStatus?.(taskId);
      if (known !== undefined && Number.isFinite(next) && known !== next)
        return [statusEvent(taskId, next)];
    }
  }
  return [];
}

const KNOWN_CAP = 500;

export function rememberStatus(known: Map<number, number>, id: number, status: number): void {
  known.delete(id);
  known.set(id, status);
  while (known.size > KNOWN_CAP) {
    const oldest = known.keys().next().value;
    if (oldest === undefined) break;
    known.delete(oldest);
  }
}

const rememberStatuses = (
  verb: string,
  path: string,
  body: Record<string, any>,
  data: unknown,
  known: Map<number, number>,
) => {
  const remember = (task: unknown) => {
    if (isRecord(task) && task.id != null && task.status_id != null) {
      const id = Number(task.id);
      const status = Number(task.status_id);
      if (Number.isFinite(id) && Number.isFinite(status)) rememberStatus(known, id, status);
    }
  };
  let m: RegExpMatchArray | null;
  if (verb === 'GET' && /^tasks(\/\d+)?$/.test(path)) {
    const payload = isRecord(data) ? data.data : null;
    if (Array.isArray(payload)) payload.forEach(remember);
    else if (isRecord(payload) && Array.isArray(payload.data)) payload.data.forEach(remember);
    else remember(payload);
  } else if ((verb === 'PUT' || verb === 'PATCH') && /^tasks\/\d+$/.test(path)) {
    remember(isRecord(data) ? data.data : null);
    if (body.status_id != null) rememberStatus(known, Number(path.split('/')[1]), Number(body.status_id));
  } else if (verb === 'PUT' && (m = path.match(/^tasks\/(\d+)\/(\d+)$/))) {
    rememberStatus(known, Number(m[1]), Number(m[2]));
  } else if (verb === 'PUT' && (m = path.match(/^statuses\/(\d+)\/tasks$/))) {
    const ids = Array.isArray(body.task_ids) ? body.task_ids : [];
    ids.forEach((id: unknown) => rememberStatus(known, Number(id), Number(m![1])));
  }
};

export function installTourInterceptor(
  axiosInstance: AxiosInstance,
  resolveStatusType: ResolveStatusType,
  bus: TourBus = tourBus,
): () => void {
  const known = new Map<number, number>();
  const id = axiosInstance.interceptors.response.use((response) => {
    try {
      const status = response?.status;
      const config = response?.config;
      if (config && (status === undefined || (status >= 200 && status < 300))) {
        const verb = (config.method ?? 'get').toUpperCase();
        const url = config.url ?? '';
        const body = parseBody(config.data);
        const events = matchRequest(verb, url, body, response.data, {
          statusType: resolveStatusType,
          knownStatus: (taskId) => known.get(taskId),
        });
        rememberStatuses(verb, normalizePath(url), body, response.data, known);
        events.forEach((e) => bus.emit(e.name, e.payload));
      }
    } catch (error) {
      console.error('tour interceptor failed', error);
    }
    return response;
  });
  return () => axiosInstance.interceptors.response.eject(id);
}
