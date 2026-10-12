import type { AxiosInstance } from 'axios';

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

export const tourBus: TourBus = {
  emit() {},
  on() {
    return () => {};
  },
};

export function matchRequest(
  method: string,
  url: string,
  body: unknown,
  responseData: unknown,
): TourEvent[] {
  void method;
  void url;
  void body;
  void responseData;
  return [];
}

export type ResolveStatusType = (statusId: number) => string | undefined;

export function installTourInterceptor(
  axiosInstance: AxiosInstance,
  resolveStatusType: ResolveStatusType,
): () => void {
  void axiosInstance;
  void resolveStatusType;
  return () => {};
}
