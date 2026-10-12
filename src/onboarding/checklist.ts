import type { Facts } from './poller';

export interface ChecklistItem {
  id: string;
  label: string;
  done: boolean;
  optional?: boolean;
  disabledMessage?: string;
}

export interface Checklist {
  items: ChecklistItem[];
  done: number;
  total: number;
}

export interface ChecklistDeps {
  isModuleEnabled(key: string): boolean;
  modulesKnown?(): boolean;
  canManageModules: boolean;
  account: 'cloud' | 'local' | 'none';
  emailVerified?: boolean;
}

export function buildChecklist(
  _intent: string | null,
  _facts: Facts,
  _deps: ChecklistDeps,
): Checklist {
  return { items: [], done: 0, total: 0 };
}
