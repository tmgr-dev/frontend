import $axios from '@/plugins/axios';
import { AxiosRequestConfig } from 'axios';
import type {
  DashboardStatistics,
  Activity,
  HeatmapData,
  RecentTask,
  TeamMemberActivity,
  DashboardData,
  ActivityFeedParams,
  HeatmapParams,
  RecentTasksParams,
  PaginatedResponse,
  ActionResult,
  ActionError,
  NetworkError,
  ValidationError,
  AuthorizationError,
  ServerError,
  TimeoutError,
  DashboardActions
} from '@/types/dashboard';

// Error handling utilities
const createActionError = (
  message: string,
  type: ActionError['type'] = 'server',
  details?: any
): ActionError => ({
  message,
  type,
  details,
  timestamp: new Date().toISOString(),
  recoverable: type === 'network' || type === 'timeout'
});

const createNetworkError = (
  message: string,
  statusCode: number,
  isOffline = false
): NetworkError => ({
  message,
  type: 'network',
  status_code: statusCode,
  is_offline: isOffline,
  timestamp: new Date().toISOString(),
  recoverable: true
});

const createValidationError = (
  message: string,
  fieldErrors: Record<string, string[]>
): ValidationError => ({
  message,
  type: 'validation',
  field_errors: fieldErrors,
  timestamp: new Date().toISOString(),
  recoverable: false
});

const createAuthorizationError = (
  message: string,
  requiredPermission?: string
): AuthorizationError => ({
  message,
  type: 'authorization',
  required_permission: requiredPermission,
  timestamp: new Date().toISOString(),
  recoverable: false
});

const createServerError = (
  message: string,
  statusCode: number,
  traceId?: string
): ServerError => ({
  message,
  type: 'server',
  status_code: statusCode,
  trace_id: traceId,
  timestamp: new Date().toISOString(),
  recoverable: statusCode >= 500
});

const createTimeoutError = (
  message: string,
  timeoutDuration: number,
  operation: string
): TimeoutError => ({
  message,
  type: 'timeout',
  timeout_duration: timeoutDuration,
  operation,
  timestamp: new Date().toISOString(),
  recoverable: true
});

// Response wrapper utility
const createActionResult = <T>(
  data?: T,
  error?: ActionError,
  statusCode?: number,
  cached = false,
  duration?: number
): ActionResult<T> => ({
  data,
  error,
  success: !error,
  status_code: statusCode,
  cached,
  duration,
  timestamp: new Date().toISOString()
});

// Enhanced response wrapper with metadata (currently unused but available for future use)
// const createDetailedActionResult = <T>(
//   data?: T,
//   error?: ActionError,
//   statusCode?: number,
//   metadata?: any
// ): DetailedActionResult<T> => ({
//   data,
//   error,
//   success: !error,
//   status: error ? 'error' : 'success',
//   status_code: statusCode,
//   timestamp: new Date().toISOString(),
//   metadata
// });

// Request configuration with defaults
const createRequestConfig = (options?: {
  timeout?: number;
  cache?: boolean;
}): AxiosRequestConfig => ({
  timeout: options?.timeout || 30000,
  headers: {
    'Accept': 'application/json',
    'Content-Type': 'application/json'
  }
});

// Error handler utility
const handleApiError = (error: any, operation: string): ActionError => {
  if (!error.response) {
    // Network error
    return createNetworkError(
      'Network connection failed. Please check your internet connection.',
      0,
      !navigator.onLine
    );
  }

  const { status, data } = error.response;

  switch (status) {
    case 400:
      return createValidationError(
        data?.message || 'Invalid request parameters',
        data?.errors || {}
      );
    case 401:
      return createAuthorizationError(
        'Authentication required. Please log in again.',
        data?.required_permission
      );
    case 403:
      return createAuthorizationError(
        'Access denied. You do not have permission to perform this action.',
        data?.required_permission
      );
    case 404:
      return createActionError(
        'The requested resource was not found.',
        'client',
        { operation, status }
      );
    case 408:
      return createTimeoutError(
        'Request timeout. The server took too long to respond.',
        30000,
        operation
      );
    case 422:
      return createValidationError(
        data?.message || 'Validation failed',
        data?.errors || {}
      );
    case 429:
      return createActionError(
        'Too many requests. Please wait before trying again.',
        'client',
        { retry_after: data?.retry_after || 60 }
      );
    case 500:
    case 502:
    case 503:
    case 504:
      return createServerError(
        data?.message || 'Server error occurred. Please try again later.',
        status,
        data?.trace_id
      );
    default:
      return createActionError(
        data?.message || `Unexpected error occurred (${status})`,
        'server',
        { operation, status }
      );
  }
};

// Dashboard API Actions Implementation

/**
 * Get dashboard statistics for a workspace
 */
export const getDashboardStatistics = async (
  workspaceId: number,
  options?: {
    cache?: boolean;
    timeout?: number;
  }
): Promise<ActionResult<DashboardStatistics>> => {
  const startTime = Date.now();
  
  try {
    const config = createRequestConfig(options);
    
    const { data, status } = await $axios.get(
      `/workspaces/${workspaceId}/dashboard/statistics`,
      config
    );

    const duration = Date.now() - startTime;
    
    return createActionResult(
      data.data,
      undefined,
      status,
      options?.cache || false,
      duration
    );
  } catch (error) {
    const actionError = handleApiError(error, 'getDashboardStatistics');
    return createActionResult<DashboardStatistics>(undefined, actionError);
  }
};

/**
 * Get activity feed with pagination
 */
export const getActivityFeed = async (
  workspaceId: number,
  params?: ActivityFeedParams,
  options?: {
    cache?: boolean;
    timeout?: number;
  }
): Promise<ActionResult<PaginatedResponse<Activity>>> => {
  const startTime = Date.now();
  
  try {
    const config = createRequestConfig(options);
    
    const queryParams = {
      page: params?.page || 1,
      per_page: params?.per_page || 20,
      ...params?.filters,
      sort_field: params?.sort?.field,
      sort_direction: params?.sort?.direction
    };

    const { data, status } = await $axios.get(
      `/workspaces/${workspaceId}/dashboard/activities`,
      {
        ...config,
        params: queryParams
      }
    );

    const duration = Date.now() - startTime;
    
    return createActionResult(
      data,
      undefined,
      status,
      options?.cache || false,
      duration
    );
  } catch (error) {
    const actionError = handleApiError(error, 'getActivityFeed');
    return createActionResult<PaginatedResponse<Activity>>(undefined, actionError);
  }
};

/**
 * Get user heatmap data
 */
export const getUserHeatmap = async (
  workspaceId: number,
  params?: HeatmapParams,
  options?: {
    cache?: boolean;
    timeout?: number;
  }
): Promise<ActionResult<HeatmapData>> => {
  const startTime = Date.now();
  
  try {
    const config = createRequestConfig(options);
    
    const queryParams = {
      year: params?.year || new Date().getFullYear(),
      user_id: params?.user_id,
      timezone: params?.timezone || Intl.DateTimeFormat().resolvedOptions().timeZone,
      include_weekends: params?.include_weekends ?? true
    };

    const { data, status } = await $axios.get(
      `/workspaces/${workspaceId}/dashboard/heatmap`,
      {
        ...config,
        params: queryParams
      }
    );

    const duration = Date.now() - startTime;
    
    return createActionResult(
      data.data,
      undefined,
      status,
      options?.cache || false,
      duration
    );
  } catch (error) {
    const actionError = handleApiError(error, 'getUserHeatmap');
    return createActionResult<HeatmapData>(undefined, actionError);
  }
};

/**
 * Get recent tasks
 */
export const getRecentTasks = async (
  workspaceId: number,
  params?: RecentTasksParams,
  options?: {
    cache?: boolean;
    timeout?: number;
  }
): Promise<ActionResult<RecentTask[]>> => {
  const startTime = Date.now();
  
  try {
    const config = createRequestConfig(options);
    
    const queryParams = {
      limit: params?.limit || 10,
      ...params?.filters,
      sort_field: params?.sort?.field,
      sort_direction: params?.sort?.direction
    };

    const { data, status } = await $axios.get(
      `/workspaces/${workspaceId}/dashboard/recent-tasks`,
      {
        ...config,
        params: queryParams
      }
    );

    const duration = Date.now() - startTime;
    
    return createActionResult(
      data.data,
      undefined,
      status,
      options?.cache || false,
      duration
    );
  } catch (error) {
    const actionError = handleApiError(error, 'getRecentTasks');
    return createActionResult<RecentTask[]>(undefined, actionError);
  }
};

/**
 * Get team activity
 */
export const getTeamActivity = async (
  workspaceId: number,
  options?: {
    cache?: boolean;
    timeout?: number;
  }
): Promise<ActionResult<TeamMemberActivity>> => {
  const startTime = Date.now();
  
  try {
    const config = createRequestConfig(options);

    const { data, status } = await $axios.get(
      `/workspaces/${workspaceId}/dashboard/team-activity`,
      config
    );

    const duration = Date.now() - startTime;
    
    return createActionResult(
      data.data,
      undefined,
      status,
      options?.cache || false,
      duration
    );
  } catch (error) {
    const actionError = handleApiError(error, 'getTeamActivity');
    return createActionResult<TeamMemberActivity>(undefined, actionError);
  }
};

/**
 * Get complete dashboard data
 */
export const getDashboardData = async (
  workspaceId: number,
  options?: {
    cache?: boolean;
    timeout?: number;
  }
): Promise<ActionResult<DashboardData>> => {
  const startTime = Date.now();
  
  try {
    const config = createRequestConfig(options);

    const { data, status } = await $axios.get(
      `/workspaces/${workspaceId}/dashboard`,
      config
    );

    const duration = Date.now() - startTime;
    
    return createActionResult(
      data.data,
      undefined,
      status,
      options?.cache || false,
      duration
    );
  } catch (error) {
    const actionError = handleApiError(error, 'getDashboardData');
    return createActionResult<DashboardData>(undefined, actionError);
  }
};

/**
 * Refresh specific dashboard section
 */
export const refreshDashboardSection = async (
  workspaceId: number,
  section: 'statistics' | 'activities' | 'heatmap' | 'tasks' | 'team',
  options?: {
    force?: boolean;
    timeout?: number;
  }
): Promise<ActionResult<any>> => {
  const startTime = Date.now();
  
  try {
    const config = createRequestConfig(options);
    
    const queryParams = {
      force: options?.force || false
    };

    const { data, status } = await $axios.post(
      `/workspaces/${workspaceId}/dashboard/refresh/${section}`,
      {},
      {
        ...config,
        params: queryParams
      }
    );

    const duration = Date.now() - startTime;
    
    return createActionResult(
      data.data,
      undefined,
      status,
      false,
      duration
    );
  } catch (error) {
    const actionError = handleApiError(error, 'refreshDashboardSection');
    return createActionResult<any>(undefined, actionError);
  }
};

// Batch operations for multiple dashboard sections
export const refreshMultipleSections = async (
  workspaceId: number,
  sections: Array<'statistics' | 'activities' | 'heatmap' | 'tasks' | 'team'>,
  options?: {
    force?: boolean;
    timeout?: number;
  }
): Promise<ActionResult<Record<string, any>>> => {
  const startTime = Date.now();
  
  try {
    const promises = sections.map(section => 
      refreshDashboardSection(workspaceId, section, options)
    );
    
    const results = await Promise.allSettled(promises);
    const duration = Date.now() - startTime;
    
    const data: Record<string, any> = {};
    const errors: Record<string, ActionError> = {};
    
    results.forEach((result, index) => {
      const section = sections[index];
      if (result.status === 'fulfilled' && result.value.success) {
        data[section] = result.value.data;
      } else if (result.status === 'fulfilled' && result.value.error) {
        errors[section] = result.value.error;
      } else if (result.status === 'rejected') {
        errors[section] = createActionError(
          `Failed to refresh ${section}`,
          'server',
          result.reason
        );
      }
    });
    
    const hasErrors = Object.keys(errors).length > 0;
    
    return createActionResult(
      data,
      hasErrors ? createActionError(
        'Some sections failed to refresh',
        'server',
        { errors, successful_sections: Object.keys(data) }
      ) : undefined,
      200,
      false,
      duration
    );
  } catch (error) {
    const actionError = handleApiError(error, 'refreshMultipleSections');
    return createActionResult<Record<string, any>>(undefined, actionError);
  }
};

// Utility functions for error handling and retry logic
export const withRetry = async <T>(
  operation: () => Promise<ActionResult<T>>,
  maxRetries = 3,
  retryDelay = 1000,
  exponentialBackoff = true
): Promise<ActionResult<T>> => {
  let lastError: ActionError | undefined;
  
  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      const result = await operation();
      
      if (result.success || !result.error?.recoverable) {
        return result;
      }
      
      lastError = result.error;
      
      if (attempt < maxRetries) {
        const delay = exponentialBackoff 
          ? retryDelay * Math.pow(2, attempt)
          : retryDelay;
        
        await new Promise(resolve => setTimeout(resolve, delay));
      }
    } catch (error) {
      lastError = createActionError(
        'Retry operation failed',
        'server',
        error
      );
    }
  }
  
  return createActionResult<T>(
    undefined,
    lastError || createActionError('Max retries exceeded', 'server')
  );
};

// Cache management utilities
const cache = new Map<string, { data: any; timestamp: number; ttl: number }>();

export const getCachedResult = <T>(key: string): T | null => {
  const entry = cache.get(key);
  if (!entry) return null;
  
  if (Date.now() - entry.timestamp > entry.ttl) {
    cache.delete(key);
    return null;
  }
  
  return entry.data;
};

export const setCachedResult = <T>(key: string, data: T, ttl = 300000): void => {
  cache.set(key, {
    data,
    timestamp: Date.now(),
    ttl
  });
};

export const clearCache = (pattern?: string): void => {
  if (!pattern) {
    cache.clear();
    return;
  }
  
  const regex = new RegExp(pattern);
  for (const key of cache.keys()) {
    if (regex.test(key)) {
      cache.delete(key);
    }
  }
};

// Dashboard actions object implementing the DashboardActions interface
export const dashboardActions: DashboardActions = {
  getDashboardStatistics,
  getActivityFeed,
  getUserHeatmap,
  getRecentTasks,
  getTeamActivity,
  getDashboardData,
  refreshDashboardSection
};