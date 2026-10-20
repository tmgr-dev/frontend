import type { RoutineEntry, ViewId, YearStats } from '@/types/dailyRoutine';
import type { LocalProfile } from '@/utils/localProfile';
import { Store } from 'vuex';

export interface UserSetting {
	id: number;
	key: string;
	name: string;
	value: string | number;
	description: string;
}

export interface User {
	desktop_app_seen_at?: string | null;
	has_avatar?: boolean;
	is_local?: boolean;
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
	is_local?: boolean;
	path?: string;
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
	pagesEvent: { type: string; page: any; seq: number } | null;
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
	modalStack: string[];
	urlManuallyChanged: boolean;
	updatedTaskData: any | null;
	updatedTaskKey: number;
	localProfile: LocalProfile | null;
}

export interface StoreState extends RootState {
	filter: BoardFilterState;
	dailyRoutines: DailyRoutinesState;
}

export interface BoardFilterState {
	selectedCategory: number;
	searchText: string | null;
	selectedUser: number;
	selectedPersona: string;
}

export interface DailyRoutinesState {
	view: ViewId;
	cursor: string;
	entries: RoutineEntry[];
	yearStats: YearStats;
	isLoading: boolean;
	rangeLoading: boolean;
	yearLoading: boolean;
	rangeError: unknown;
	yearError: unknown;
	lastRange: { from: string | null; to: string | null };
}

export type AccountMode = 'cloud' | 'local' | 'none';

export interface RootGetters {
	isLoggedIn: boolean;
	accountMode: AccountMode;
	canUseApp: boolean;
	isLocalOnly: boolean;
	workspaceById: (id: number) => Workspace | undefined;
	workspaceStatusById: (id: number) => WorkspaceStatus | undefined;
	userSettingByKey: (key: string) => UserSetting | undefined;
	currentWorkspaceId: number | null;
	currentWorkspace: Workspace | null;
}

declare module 'vuex' {
	export function useStore(): Store<StoreState>;
}

declare module '@vue/runtime-core' {
	interface ComponentCustomProperties {
		$store: Store<StoreState>;
	}
}

export {};
