import { Store } from 'vuex';

export interface UserSetting {
  id: number;
  key: string;
  name: string;
  value: string | number;
  description: string;
}

export interface User {
  id: number;
  email: string;
  role: number;
  name: string;
  settings: UserSetting[];
}

export interface Workspace {
  id: number;
  name: string;
  code: string;
  is_default: boolean;
  type: string;
  user_id: number;
}

export interface WorkspaceStatus {
  id: number;
  name: string;
  color?: string;
  order?: number;
}

export interface UserSettings {
  showTooltips: boolean;
  colorScheme?: string;
}

export interface RootState {
  metaTitle: string;
  token: string | null;
  user: User | Record<string, never>;
  colorScheme: string;
  currentTaskIdForModal: number | null;
  createTaskInProjectCategoryId: number | null;
  taskStatusId: number | null;
  showCreatingTaskModal: boolean;
  reloadActiveTasksKey: number;
  reloadTasksKey: number;
  appRerenderKey: number;
  workspaceStatuses: WorkspaceStatus[];
  workspaceStatusesById: Record<number, WorkspaceStatus>;
  workspaces: Workspace[];
  workspacesById: Record<number, Workspace>;
  userSettingsMap: Record<string, UserSetting>;
  userSettings: UserSettings;
  openModals: number;
  urlManuallyChanged: boolean;
}

export interface RootGetters {
  isLoggedIn: boolean;
  workspaceById: (id: number) => Workspace | undefined;
  workspaceStatusById: (id: number) => WorkspaceStatus | undefined;
  userSettingByKey: (key: string) => UserSetting | undefined;
  currentWorkspaceId: number | null;
  currentWorkspace: Workspace | null;
}

declare module 'vuex' {
  export function useStore(): Store<RootState>;
}

declare module '@vue/runtime-core' {
  interface ComponentCustomProperties {
    $store: Store<RootState>;
  }
}

export {};
