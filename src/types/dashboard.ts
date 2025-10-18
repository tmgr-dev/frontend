// Dashboard TypeScript interfaces and types

// Activity Types Enum
export enum ActivityType {
  // Task activities
  TASK_CREATED = 'task_created',
  TASK_UPDATED = 'task_updated',
  TASK_COMPLETED = 'task_completed',
  TASK_DELETED = 'task_deleted',
  TASK_RESTORED = 'task_restored',
  TASK_STATUS_CHANGED = 'task_status_changed',
  TASK_ASSIGNED = 'task_assigned',
  TASK_TIMER_STARTED = 'task_timer_started',
  TASK_TIMER_STOPPED = 'task_timer_stopped',
  
  // Comment activities
  COMMENT_CREATED = 'comment_created',
  COMMENT_UPDATED = 'comment_updated',
  COMMENT_DELETED = 'comment_deleted',
  
  // Category activities
  CATEGORY_CREATED = 'category_created',
  CATEGORY_UPDATED = 'category_updated',
  CATEGORY_DELETED = 'category_deleted',
  
  // File activities
  FILE_UPLOADED = 'file_uploaded',
  FILE_DELETED = 'file_deleted',
  
  // Workspace activities
  MEMBER_JOINED = 'member_joined',
  MEMBER_LEFT = 'member_left',
  
  // Daily routine activities
  ROUTINE_COMPLETED = 'routine_completed'
}

// Dashboard Statistics Interface
export interface DashboardStatistics {
  total_tasks: number;
  active_tasks: number;
  completed_today: number;
  completed_week: number;
  completed_month: number;
  time_today: number;
  time_week: number;
  time_month: number;
  time_total: number;
  team_members: number;
  overdue_tasks: number;
  daily_routine_completion_rate: number;
  average_task_completion_time?: number;
}

// User Interface for Activities
export interface ActivityUser {
  id: number;
  name: string;
  email?: string;
  avatar?: string;
}

// Activity Interface
export interface Activity {
  id: number;
  type: ActivityType | string;
  title: string;
  subject_name: string;
  description: string;
  user: ActivityUser;
  workspace_id: number;
  subject_type?: string;
  subject_id?: number;
  metadata: Record<string, any>;
  created_at: string;
  timestamp_human: string;
}

// Contribution Data for Heatmap
export interface ContributionData {
  date: string;
  count: number;
  level: number; // 0-4 for intensity levels
}

// Streak Information
export interface StreakInfo {
  current: number;
  longest: number;
  current_start_date?: string;
  longest_start_date?: string;
  longest_end_date?: string;
}

// Heatmap Data Interface
export interface HeatmapData {
  contributions: ContributionData[];
  total_contributions: number;
  streak: StreakInfo;
  year: number;
  weeks: number;
}

// Team Member Status
export interface TeamMemberStatus {
  id: number;
  name: string;
  email: string;
  avatar?: string;
  is_online: boolean;
  last_activity_at?: string;
  current_task?: {
    id: number;
    title: string;
    timer_running: boolean;
    timer_started_at?: string;
  };
}

// Team Member Activity Interface
export interface TeamMemberActivity {
  members: TeamMemberStatus[];
  total_members: number;
  online_members: number;
  active_timers: number;
}

// Recent Task Interface
export interface RecentTask {
  id: number;
  title: string;
  description?: string;
  status?: {
    id: number;
    name: string;
    type: string;
    color?: string;
  };
  category?: {
    id: number;
    name: string;
    color?: string;
  };
  user: ActivityUser;
  assignees?: ActivityUser[];
  updated_at: string;
  updated_at_human: string;
  timer_running: boolean;
  timer_started_at?: string;
  priority?: 'low' | 'medium' | 'high' | 'urgent';
  expired_at?: string;
  is_overdue: boolean;
}

// Dashboard Data Aggregation Interface
export interface DashboardData {
  statistics: DashboardStatistics;
  recent_activities: Activity[];
  heatmap_data: HeatmapData;
  recent_tasks: RecentTask[];
  team_activity: TeamMemberActivity;
}

// API Response Types

// Generic API Response
export interface ApiResponse<T = any> {
  data: T;
  message?: string;
  success: boolean;
  timestamp?: string;
  version?: string;
}

// Pagination Meta Information
export interface PaginationMeta {
  current_page: number;
  last_page: number;
  per_page: number;
  total: number;
  from: number;
  to: number;
  path: string;
  first_page_url: string;
  last_page_url: string;
  next_page_url?: string;
  prev_page_url?: string;
}

// Pagination Links
export interface PaginationLinks {
  first: string;
  last: string;
  prev?: string;
  next?: string;
}

// Paginated Response
export interface PaginatedResponse<T = any> {
  data: T[];
  meta: PaginationMeta;
  links: PaginationLinks;
}

// API Error Response
export interface ApiErrorResponse {
  error: string;
  message: string;
  errors?: Record<string, string[]>;
  status_code: number;
  timestamp?: string;
  trace_id?: string;
  request_id?: string;
}

// Validation Error Response
export interface ValidationErrorResponse extends ApiErrorResponse {
  errors: Record<string, string[]>;
}

// Network Error Response
export interface NetworkErrorResponse {
  error: 'network_error';
  message: string;
  status_code: 0;
  timestamp: string;
  is_offline?: boolean;
  retry_after?: number;
}

// Loading State Types
export interface LoadingState {
  isLoading: boolean;
  error?: string | null;
  lastUpdated?: string;
  progress?: number; // 0-100 for progress indication
  retryCount?: number;
  canRetry?: boolean;
}

// Enhanced Loading State with Status
export interface DetailedLoadingState extends LoadingState {
  status: 'idle' | 'loading' | 'success' | 'error' | 'retrying';
  startTime?: number;
  endTime?: number;
  duration?: number;
}

// Specific Loading States for Dashboard
export interface DashboardLoadingStates {
  statistics: DetailedLoadingState;
  activities: DetailedLoadingState;
  heatmap: DetailedLoadingState;
  recentTasks: DetailedLoadingState;
  teamActivity: DetailedLoadingState;
  loadingMore: boolean;
  initialLoad: boolean;
  refreshing: boolean;
}

// Loading State Factory
export interface LoadingStateFactory {
  createInitialState: () => DetailedLoadingState;
  createLoadingState: (progress?: number) => DetailedLoadingState;
  createSuccessState: (duration?: number) => DetailedLoadingState;
  createErrorState: (error: string, canRetry?: boolean) => DetailedLoadingState;
}

// Filter Options for Activities
export interface ActivityFilters {
  type?: ActivityType | 'all';
  user_id?: number | 'all';
  date_from?: string;
  date_to?: string;
  subject_type?: string;
  search?: string;
  limit?: number;
}

// Advanced Activity Filters
export interface AdvancedActivityFilters extends ActivityFilters {
  exclude_types?: ActivityType[];
  include_metadata?: boolean;
  group_by?: 'date' | 'user' | 'type';
  time_range?: 'today' | 'week' | 'month' | 'year' | 'custom';
}

// Sort Options
export interface SortOption {
  field: string;
  direction: 'asc' | 'desc';
}

// Multiple Sort Options
export interface MultiSortOption {
  sorts: SortOption[];
}

// Predefined Sort Options
export type ActivitySortField = 'created_at' | 'type' | 'user_name';
export type TaskSortField = 'updated_at' | 'created_at' | 'title' | 'priority' | 'status';
export type TeamSortField = 'name' | 'last_activity_at' | 'is_online';

export interface TypedSortOption<T extends string> {
  field: T;
  direction: 'asc' | 'desc';
}

// Filter and Sort for Recent Tasks
export interface TaskFilters {
  status_id?: number | 'all';
  category_id?: number | 'all';
  user_id?: number | 'all';
  priority?: 'low' | 'medium' | 'high' | 'urgent' | 'all';
  is_overdue?: boolean;
  has_timer?: boolean;
  search?: string;
  assigned_to_me?: boolean;
  created_by_me?: boolean;
}

// Advanced Task Filters
export interface AdvancedTaskFilters extends TaskFilters {
  date_range?: DateRange;
  completion_status?: 'completed' | 'incomplete' | 'all';
  has_attachments?: boolean;
  has_comments?: boolean;
  tags?: string[];
}

// Request Parameters for API calls
export interface ActivityFeedParams {
  page?: number;
  per_page?: number;
  filters?: ActivityFilters;
  sort?: SortOption;
}

export interface RecentTasksParams {
  limit?: number;
  filters?: TaskFilters;
  sort?: SortOption;
}

export interface HeatmapParams {
  year?: number;
  user_id?: number;
  timezone?: string;
  include_weekends?: boolean;
}

// Base Request Parameters
export interface BaseRequestParams {
  workspace_id?: number;
  include_deleted?: boolean;
  include_metadata?: boolean;
  timezone?: string;
}

// Request Parameters with Pagination
export interface PaginatedRequestParams extends BaseRequestParams {
  page?: number;
  per_page?: number;
  cursor?: string; // For cursor-based pagination
}

// Request Parameters with Filtering and Sorting
export interface FilteredRequestParams<TFilter = any, TSort = SortOption> extends PaginatedRequestParams {
  filters?: TFilter;
  sort?: TSort | TSort[];
  search?: string;
}

// Dashboard API Response Types
export interface DashboardStatisticsResponse extends ApiResponse<DashboardStatistics> {}

export interface ActivityFeedResponse extends ApiResponse<PaginatedResponse<Activity>> {}

export interface HeatmapDataResponse extends ApiResponse<HeatmapData> {}

export interface RecentTasksResponse extends ApiResponse<RecentTask[]> {}

export interface TeamActivityResponse extends ApiResponse<TeamMemberActivity> {}

export interface DashboardDataResponse extends ApiResponse<DashboardData> {}

// Batch Response Types
export interface BatchResponse<T = any> {
  success: boolean;
  results: Array<{
    id: string | number;
    success: boolean;
    data?: T;
    error?: ApiErrorResponse;
  }>;
  total: number;
  successful: number;
  failed: number;
}

// Streaming Response Types
export interface StreamResponse<T = any> {
  chunk: T;
  sequence: number;
  total_chunks?: number;
  is_final: boolean;
  timestamp: string;
}

// Cache Response Headers
export interface CacheHeaders {
  'cache-control'?: string;
  'etag'?: string;
  'last-modified'?: string;
  'expires'?: string;
}

// Response with Cache Information
export interface CachedApiResponse<T = any> extends ApiResponse<T> {
  cache?: {
    hit: boolean;
    ttl: number;
    key: string;
    created_at: string;
  };
  headers?: CacheHeaders;
}

// Dashboard Action Types and Function Signatures

// Error Handling Types
export interface ActionError {
  message: string;
  code?: string | number;
  details?: any;
  timestamp: string;
  type?: 'network' | 'validation' | 'authorization' | 'server' | 'client' | 'timeout';
  recoverable?: boolean;
  retry_after?: number;
}

// Specific Error Types
export interface NetworkError extends ActionError {
  type: 'network';
  is_offline?: boolean;
  status_code: number;
}

export interface ValidationError extends ActionError {
  type: 'validation';
  field_errors: Record<string, string[]>;
}

export interface AuthorizationError extends ActionError {
  type: 'authorization';
  required_permission?: string;
  current_role?: string;
}

export interface ServerError extends ActionError {
  type: 'server';
  status_code: number;
  trace_id?: string;
}

export interface TimeoutError extends ActionError {
  type: 'timeout';
  timeout_duration: number;
  operation: string;
}

export interface RetryConfig {
  maxRetries: number;
  retryDelay: number;
  exponentialBackoff: boolean;
}

export interface ActionResult<T = any> {
  data?: T;
  error?: ActionError;
  success: boolean;
  status_code?: number;
  cached?: boolean;
  duration?: number;
  timestamp: string;
}

// Response Status Types
export type ResponseStatus = 
  | 'success' 
  | 'error' 
  | 'loading' 
  | 'idle' 
  | 'timeout' 
  | 'cancelled' 
  | 'retrying';

// Enhanced Action Result with Status
export interface DetailedActionResult<T = any> extends ActionResult<T> {
  status: ResponseStatus;
  metadata?: {
    request_id?: string;
    trace_id?: string;
    cache_key?: string;
    retry_count?: number;
    source?: 'cache' | 'network' | 'fallback';
  };
}

// Dashboard API Action Function Signatures
export interface DashboardActions {
  // Get dashboard statistics
  getDashboardStatistics: (
    workspaceId: number,
    options?: {
      cache?: boolean;
      timeout?: number;
    }
  ) => Promise<ActionResult<DashboardStatistics>>;

  // Get activity feed with pagination
  getActivityFeed: (
    workspaceId: number,
    params?: ActivityFeedParams,
    options?: {
      cache?: boolean;
      timeout?: number;
    }
  ) => Promise<ActionResult<PaginatedResponse<Activity>>>;

  // Get user heatmap data
  getUserHeatmap: (
    workspaceId: number,
    params?: HeatmapParams,
    options?: {
      cache?: boolean;
      timeout?: number;
    }
  ) => Promise<ActionResult<HeatmapData>>;

  // Get recent tasks
  getRecentTasks: (
    workspaceId: number,
    params?: RecentTasksParams,
    options?: {
      cache?: boolean;
      timeout?: number;
    }
  ) => Promise<ActionResult<RecentTask[]>>;

  // Get team activity
  getTeamActivity: (
    workspaceId: number,
    options?: {
      cache?: boolean;
      timeout?: number;
    }
  ) => Promise<ActionResult<TeamMemberActivity>>;

  // Get complete dashboard data
  getDashboardData: (
    workspaceId: number,
    options?: {
      cache?: boolean;
      timeout?: number;
    }
  ) => Promise<ActionResult<DashboardData>>;

  // Refresh specific dashboard section
  refreshDashboardSection: (
    workspaceId: number,
    section: 'statistics' | 'activities' | 'heatmap' | 'tasks' | 'team',
    options?: {
      force?: boolean;
      timeout?: number;
    }
  ) => Promise<ActionResult<any>>;
}

// Request Configuration Interface
export interface RequestConfig {
  timeout?: number;
  retries?: RetryConfig;
  cache?: {
    enabled: boolean;
    ttl?: number; // Time to live in seconds
    key?: string;
  };
  headers?: Record<string, string>;
}

// Cache Management Types
export interface CacheEntry<T = any> {
  data: T;
  timestamp: number;
  ttl: number;
  key: string;
}

export interface CacheManager {
  get: <T>(key: string) => CacheEntry<T> | null;
  set: <T>(key: string, data: T, ttl?: number) => void;
  delete: (key: string) => void;
  clear: () => void;
  isExpired: (entry: CacheEntry) => boolean;
}

// Real-time Event Types
export interface DashboardEvent {
  type: 'activity_created' | 'dashboard_updated' | 'task_updated' | 'member_status_changed';
  payload: any;
  workspace_id: number;
  timestamp: string;
}

export interface EventHandlers {
  onActivityCreated?: (activity: Activity) => void;
  onDashboardUpdated?: (statistics: Partial<DashboardStatistics>) => void;
  onTaskUpdated?: (task: RecentTask) => void;
  onMemberStatusChanged?: (member: TeamMemberStatus) => void;
  onError?: (error: ActionError) => void;
  onReconnect?: () => void;
}

// Composable Return Types
export interface UseDashboardReturn {
  // State
  statistics: Ref<DashboardStatistics | null>;
  activities: Ref<Activity[]>;
  heatmapData: Ref<HeatmapData | null>;
  recentTasks: Ref<RecentTask[]>;
  teamActivity: Ref<TeamMemberActivity | null>;
  
  // Loading states
  loadingStates: Ref<DashboardLoadingStates>;
  
  // Error states
  error: Ref<ActionError | null>;
  
  // Actions
  loadDashboard: () => Promise<void>;
  loadMoreActivities: () => Promise<void>;
  loadHeatmap: (params?: HeatmapParams) => Promise<void>;
  refreshSection: (section: keyof Omit<DashboardLoadingStates, 'loadingMore' | 'initialLoad' | 'refreshing'>) => Promise<void>;
  refreshDashboard: () => Promise<void>;
  clearError: () => void;
  
  // Real-time update methods
  addActivity: (activity: Activity) => void;
  updateStatistics: (newStats: Partial<DashboardStatistics>) => void;
  updateTeamMemberStatus: (memberId: number, updates: Partial<TeamMemberActivity>) => void;
  
  // Computed
  hasError: ComputedRef<boolean>;
  isAnyLoading: ComputedRef<boolean>;
  canLoadMore: ComputedRef<boolean>;
  isInitialLoad: ComputedRef<boolean>;
  
  // Utility
  cleanup: () => void;
}

export interface UsePusherReturn {
  // Connection state
  isConnected: Ref<boolean>;
  connectionState: Ref<'connecting' | 'connected' | 'disconnected' | 'error' | 'reconnecting'>;
  connectionError: Ref<ActionError | null>;
  
  // Actions
  subscribe: (channel: string, events: EventHandlers) => void;
  unsubscribe: (channel: string) => void;
  unsubscribeAll: () => void;
  reconnect: () => void;
  disconnect: () => void;
  emit: (event: string, data: any) => void;
  
  // Convenience methods
  subscribeToWorkspace: (workspaceId: number, events: EventHandlers) => void;
  subscribeToUser: (userId: number, events: EventHandlers) => void;
  unsubscribeFromWorkspace: (workspaceId: number) => void;
  unsubscribeFromUser: (userId: number) => void;
  
  // Utility
  getConnectionInfo: () => any;
}

// Type guards for runtime type checking
export const isActivity = (obj: any): obj is Activity => {
  return obj && 
    typeof obj.id === 'number' &&
    typeof obj.type === 'string' &&
    typeof obj.description === 'string' &&
    obj.user &&
    typeof obj.user.id === 'number';
};

export const isDashboardStatistics = (obj: any): obj is DashboardStatistics => {
  return obj &&
    typeof obj.total_tasks === 'number' &&
    typeof obj.active_tasks === 'number' &&
    typeof obj.completed_today === 'number';
};

export const isApiErrorResponse = (obj: any): obj is ApiErrorResponse => {
  return obj &&
    typeof obj.error === 'string' &&
    typeof obj.message === 'string' &&
    typeof obj.status_code === 'number';
};

export const isValidationErrorResponse = (obj: any): obj is ValidationErrorResponse => {
  return isApiErrorResponse(obj) &&
    obj.errors &&
    typeof obj.errors === 'object';
};

export const isNetworkErrorResponse = (obj: any): obj is NetworkErrorResponse => {
  return obj &&
    obj.error === 'network_error' &&
    typeof obj.message === 'string' &&
    obj.status_code === 0;
};

export const isPaginatedResponse = <T>(obj: any): obj is PaginatedResponse<T> => {
  return obj &&
    Array.isArray(obj.data) &&
    obj.meta &&
    typeof obj.meta.current_page === 'number' &&
    typeof obj.meta.total === 'number';
};

export const isLoadingState = (obj: any): obj is LoadingState => {
  return obj &&
    typeof obj.isLoading === 'boolean';
};

export const isDetailedLoadingState = (obj: any): obj is DetailedLoadingState => {
  return obj &&
    typeof obj.isLoading === 'boolean' &&
    typeof obj.status === 'string' &&
    ['idle', 'loading', 'success', 'error', 'retrying'].includes(obj.status);
};

export const isActionResult = <T>(obj: any): obj is ActionResult<T> => {
  return obj &&
    typeof obj.success === 'boolean' &&
    typeof obj.timestamp === 'string';
};

// Utility types
export type DashboardSection = keyof DashboardLoadingStates;
export type ActivityTypeFilter = ActivityType | 'all';
export type UserFilter = number | 'all';
export type DateRange = { from: string; to: string };

// Response Type Utilities
export type ApiResponseData<T extends ApiResponse<any>> = T extends ApiResponse<infer U> ? U : never;
export type PaginatedData<T extends PaginatedResponse<any>> = T extends PaginatedResponse<infer U> ? U : never;

// Filter Type Utilities
export type FilterValue<T> = T | 'all' | undefined;
export type OptionalFilter<T> = Partial<T>;
export type RequiredFilter<T, K extends keyof T> = T & Required<Pick<T, K>>;

// Request State Types
export type RequestState = 'idle' | 'pending' | 'fulfilled' | 'rejected';

// Generic Response Wrapper
export interface ResponseWrapper<T = any> {
  response: Response;
  data: T;
  headers: Headers;
  status: number;
  statusText: string;
  url: string;
}

// API Endpoint Configuration
export interface EndpointConfig {
  url: string;
  method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  timeout?: number;
  retries?: number;
  cache?: boolean;
  auth_required?: boolean;
}

// Re-export Vue types for convenience
import type { Ref, ComputedRef } from 'vue';