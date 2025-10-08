// Example usage of dashboard API response types
import type {
  ApiResponse,
  PaginatedResponse,
  ApiErrorResponse,
  ValidationErrorResponse,
  NetworkErrorResponse,
  DashboardStatistics,
  Activity,
  HeatmapData,
  RecentTask,
  TeamMemberActivity,
  ActivityFilters,
  TaskFilters,
  SortOption,
  LoadingState,
  DetailedLoadingState,
  ActionResult,
  DetailedActionResult,
  // Response types
  DashboardStatisticsResponse,
  ActivityFeedResponse,
  HeatmapDataResponse,
  RecentTasksResponse,
  TeamActivityResponse
} from '../dashboard';

// Import enums and type guards as regular imports (not type imports)
import {
  ActivityType,
  isApiErrorResponse,
  isPaginatedResponse,
  isValidationErrorResponse,
  isNetworkErrorResponse
} from '../dashboard';

// Example: Handling API responses with proper typing

// 1. Dashboard Statistics API Response
export function handleDashboardStatistics(response: unknown): DashboardStatistics | null {
  // Type guard to ensure response structure
  if (typeof response === 'object' && response !== null && 'data' in response) {
    const apiResponse = response as ApiResponse<DashboardStatistics>;
    if (apiResponse.success && apiResponse.data) {
      return apiResponse.data;
    }
  }
  return null;
}

// 2. Activity Feed with Pagination
export function handleActivityFeed(response: unknown): Activity[] | null {
  if (typeof response === 'object' && response !== null && 'data' in response) {
    const apiResponse = response as ApiResponse<PaginatedResponse<Activity>>;
    if (apiResponse.success && isPaginatedResponse(apiResponse.data)) {
      return apiResponse.data.data;
    }
  }
  return null;
}

// 3. Error Handling with Type Guards
export function handleApiError(error: unknown): string {
  if (isApiErrorResponse(error)) {
    if (isValidationErrorResponse(error)) {
      // Handle validation errors
      const fieldErrors = Object.entries(error.errors)
        .map(([field, messages]) => `${field}: ${messages.join(', ')}`)
        .join('; ');
      return `Validation failed: ${fieldErrors}`;
    }
    
    if (isNetworkErrorResponse(error)) {
      // Handle network errors
      return error.is_offline 
        ? 'You appear to be offline. Please check your connection.'
        : 'Network error occurred. Please try again.';
    }
    
    // Generic API error
    return error.message || 'An error occurred';
  }
  
  return 'Unknown error occurred';
}

// 4. Loading State Management
export function createLoadingStateManager() {
  const states = new Map<string, DetailedLoadingState>();
  
  const createInitialState = (): DetailedLoadingState => ({
    isLoading: false,
    status: 'idle',
    error: null,
    canRetry: true,
    retryCount: 0
  });
  
  const setLoading = (key: string, progress?: number): DetailedLoadingState => {
    const state: DetailedLoadingState = {
      isLoading: true,
      status: 'loading',
      error: null,
      progress,
      startTime: Date.now(),
      canRetry: false,
      retryCount: states.get(key)?.retryCount || 0
    };
    states.set(key, state);
    return state;
  };
  
  const setSuccess = (key: string): DetailedLoadingState => {
    const currentState = states.get(key) || createInitialState();
    const state: DetailedLoadingState = {
      ...currentState,
      isLoading: false,
      status: 'success',
      error: null,
      endTime: Date.now(),
      duration: currentState.startTime ? Date.now() - currentState.startTime : undefined,
      canRetry: true
    };
    states.set(key, state);
    return state;
  };
  
  const setError = (key: string, error: string, canRetry = true): DetailedLoadingState => {
    const currentState = states.get(key) || createInitialState();
    const state: DetailedLoadingState = {
      ...currentState,
      isLoading: false,
      status: 'error',
      error,
      endTime: Date.now(),
      duration: currentState.startTime ? Date.now() - currentState.startTime : undefined,
      canRetry,
      retryCount: (currentState.retryCount || 0) + 1
    };
    states.set(key, state);
    return state;
  };
  
  return {
    createInitialState,
    setLoading,
    setSuccess,
    setError,
    getState: (key: string) => states.get(key) || createInitialState(),
    getAllStates: () => new Map(states)
  };
}

// 5. API Request with Filters and Sorting
export interface DashboardApiClient {
  getStatistics(workspaceId: number): Promise<ActionResult<DashboardStatistics>>;
  getActivities(
    workspaceId: number, 
    filters?: ActivityFilters, 
    sort?: SortOption,
    page?: number
  ): Promise<ActionResult<PaginatedResponse<Activity>>>;
  getRecentTasks(
    workspaceId: number,
    filters?: TaskFilters,
    limit?: number
  ): Promise<ActionResult<RecentTask[]>>;
}

// Example implementation with proper error handling
export class DashboardApiClientImpl implements DashboardApiClient {
  private baseUrl: string;
  private loadingManager = createLoadingStateManager();
  
  constructor(baseUrl: string) {
    this.baseUrl = baseUrl;
  }
  
  async getStatistics(workspaceId: number): Promise<ActionResult<DashboardStatistics>> {
    const key = `statistics-${workspaceId}`;
    this.loadingManager.setLoading(key);
    
    try {
      const response = await fetch(`${this.baseUrl}/workspaces/${workspaceId}/dashboard/statistics`);
      
      if (!response.ok) {
        const errorData = await response.json();
        this.loadingManager.setError(key, handleApiError(errorData));
        
        return {
          success: false,
          error: {
            message: handleApiError(errorData),
            code: response.status,
            timestamp: new Date().toISOString(),
            type: response.status >= 500 ? 'server' : 'client'
          },
          timestamp: new Date().toISOString()
        };
      }
      
      const data: DashboardStatisticsResponse = await response.json();
      this.loadingManager.setSuccess(key);
      
      return {
        success: true,
        data: data.data,
        timestamp: new Date().toISOString(),
        cached: false
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Network error';
      this.loadingManager.setError(key, errorMessage);
      
      return {
        success: false,
        error: {
          message: errorMessage,
          timestamp: new Date().toISOString(),
          type: 'network'
        },
        timestamp: new Date().toISOString()
      };
    }
  }
  
  async getActivities(
    workspaceId: number,
    filters?: ActivityFilters,
    sort?: SortOption,
    page = 1
  ): Promise<ActionResult<PaginatedResponse<Activity>>> {
    const key = `activities-${workspaceId}`;
    this.loadingManager.setLoading(key);
    
    try {
      const params = new URLSearchParams({
        page: page.toString(),
        ...(filters?.type && filters.type !== 'all' && { type: filters.type }),
        ...(filters?.user_id && filters.user_id !== 'all' && { user_id: filters.user_id.toString() }),
        ...(filters?.date_from && { date_from: filters.date_from }),
        ...(filters?.date_to && { date_to: filters.date_to }),
        ...(filters?.search && { search: filters.search }),
        ...(sort && { sort_field: sort.field, sort_direction: sort.direction })
      });
      
      const response = await fetch(
        `${this.baseUrl}/workspaces/${workspaceId}/dashboard/activities?${params}`
      );
      
      if (!response.ok) {
        const errorData = await response.json();
        this.loadingManager.setError(key, handleApiError(errorData));
        
        return {
          success: false,
          error: {
            message: handleApiError(errorData),
            code: response.status,
            timestamp: new Date().toISOString(),
            type: response.status >= 500 ? 'server' : 'client'
          },
          timestamp: new Date().toISOString()
        };
      }
      
      const data: ActivityFeedResponse = await response.json();
      this.loadingManager.setSuccess(key);
      
      return {
        success: true,
        data: data.data,
        timestamp: new Date().toISOString(),
        cached: false
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Network error';
      this.loadingManager.setError(key, errorMessage);
      
      return {
        success: false,
        error: {
          message: errorMessage,
          timestamp: new Date().toISOString(),
          type: 'network'
        },
        timestamp: new Date().toISOString()
      };
    }
  }
  
  async getRecentTasks(
    workspaceId: number,
    filters?: TaskFilters,
    limit = 10
  ): Promise<ActionResult<RecentTask[]>> {
    const key = `recent-tasks-${workspaceId}`;
    this.loadingManager.setLoading(key);
    
    try {
      const params = new URLSearchParams({
        limit: limit.toString(),
        ...(filters?.status_id && filters.status_id !== 'all' && { status_id: filters.status_id.toString() }),
        ...(filters?.category_id && filters.category_id !== 'all' && { category_id: filters.category_id.toString() }),
        ...(filters?.priority && filters.priority !== 'all' && { priority: filters.priority }),
        ...(filters?.is_overdue !== undefined && { is_overdue: filters.is_overdue.toString() }),
        ...(filters?.has_timer !== undefined && { has_timer: filters.has_timer.toString() }),
        ...(filters?.search && { search: filters.search })
      });
      
      const response = await fetch(
        `${this.baseUrl}/workspaces/${workspaceId}/dashboard/recent-tasks?${params}`
      );
      
      if (!response.ok) {
        const errorData = await response.json();
        this.loadingManager.setError(key, handleApiError(errorData));
        
        return {
          success: false,
          error: {
            message: handleApiError(errorData),
            code: response.status,
            timestamp: new Date().toISOString(),
            type: response.status >= 500 ? 'server' : 'client'
          },
          timestamp: new Date().toISOString()
        };
      }
      
      const data: RecentTasksResponse = await response.json();
      this.loadingManager.setSuccess(key);
      
      return {
        success: true,
        data: data.data,
        timestamp: new Date().toISOString(),
        cached: false
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Network error';
      this.loadingManager.setError(key, errorMessage);
      
      return {
        success: false,
        error: {
          message: errorMessage,
          timestamp: new Date().toISOString(),
          type: 'network'
        },
        timestamp: new Date().toISOString()
      };
    }
  }
  
  getLoadingState(key: string): DetailedLoadingState {
    return this.loadingManager.getState(key);
  }
}

// 6. Usage Example with New Dashboard Actions
export async function exampleUsage() {
  const apiClient = new DashboardApiClientImpl('/api');
  const workspaceId = 123;
  
  // Get statistics
  const statsResult = await apiClient.getStatistics(workspaceId);
  if (statsResult.success && statsResult.data) {
    console.log('Total tasks:', statsResult.data.total_tasks);
    console.log('Active tasks:', statsResult.data.active_tasks);
  } else if (statsResult.error) {
    console.error('Failed to load statistics:', statsResult.error.message);
  }
  
  // Get activities with filters
  const activitiesResult = await apiClient.getActivities(workspaceId, {
    type: ActivityType.TASK_CREATED,
    date_from: '2024-01-01',
    search: 'important'
  }, {
    field: 'created_at',
    direction: 'desc'
  });
  
  if (activitiesResult.success && activitiesResult.data) {
    console.log('Activities:', activitiesResult.data.data);
    console.log('Total pages:', activitiesResult.data.meta.last_page);
  }
  
  // Get recent tasks with filters
  const tasksResult = await apiClient.getRecentTasks(workspaceId, {
    priority: 'high',
    is_overdue: true
  }, 5);
  
  if (tasksResult.success && tasksResult.data) {
    console.log('Overdue high priority tasks:', tasksResult.data);
  }
}

// 7. Usage Example with New Dashboard Actions from dashboard.ts
import {
  getDashboardStatistics,
  getActivityFeed,
  getUserHeatmap,
  getRecentTasks,
  getTeamActivity,
  getDashboardData,
  refreshDashboardSection,
  withRetry
} from '../../actions/tmgr/dashboard';

export async function exampleDashboardActionsUsage() {
  const workspaceId = 123;
  
  // Example 1: Get dashboard statistics with error handling
  const statsResult = await getDashboardStatistics(workspaceId, {
    cache: true,
    timeout: 10000
  });
  
  if (statsResult.success && statsResult.data) {
    console.log('Dashboard Statistics:', {
      totalTasks: statsResult.data.total_tasks,
      activeTasks: statsResult.data.active_tasks,
      completedToday: statsResult.data.completed_today,
      timeToday: statsResult.data.time_today
    });
  } else if (statsResult.error) {
    console.error('Statistics Error:', {
      type: statsResult.error.type,
      message: statsResult.error.message,
      recoverable: statsResult.error.recoverable
    });
  }
  
  // Example 2: Get activity feed with pagination and filters
  const activitiesResult = await getActivityFeed(workspaceId, {
    page: 1,
    per_page: 20,
    filters: {
      type: ActivityType.TASK_COMPLETED,
      date_from: '2024-01-01',
      user_id: 'all'
    },
    sort: {
      field: 'created_at',
      direction: 'desc'
    }
  });
  
  if (activitiesResult.success && activitiesResult.data) {
    console.log('Activity Feed:', {
      activities: activitiesResult.data.data,
      pagination: activitiesResult.data.meta,
      cached: activitiesResult.cached,
      duration: activitiesResult.duration
    });
  }
  
  // Example 3: Get user heatmap data
  const heatmapResult = await getUserHeatmap(workspaceId, {
    year: 2024,
    timezone: 'America/New_York',
    include_weekends: true
  });
  
  if (heatmapResult.success && heatmapResult.data) {
    console.log('Heatmap Data:', {
      totalContributions: heatmapResult.data.total_contributions,
      currentStreak: heatmapResult.data.streak.current,
      longestStreak: heatmapResult.data.streak.longest,
      contributionsCount: heatmapResult.data.contributions.length
    });
  }
  
  // Example 4: Get recent tasks with filters
  const recentTasksResult = await getRecentTasks(workspaceId, {
    limit: 5,
    filters: {
      priority: 'high',
      is_overdue: false,
      has_timer: true
    },
    sort: {
      field: 'updated_at',
      direction: 'desc'
    }
  });
  
  if (recentTasksResult.success && recentTasksResult.data) {
    console.log('Recent Tasks:', recentTasksResult.data.map(task => ({
      id: task.id,
      title: task.title,
      status: task.status?.name || 'Unknown',
      timerRunning: task.timer_running,
      isOverdue: task.is_overdue
    })));
  }
  
  // Example 5: Get team activity
  const teamResult = await getTeamActivity(workspaceId);
  
  if (teamResult.success && teamResult.data) {
    console.log('Team Activity:', {
      totalMembers: teamResult.data.total_members,
      onlineMembers: teamResult.data.online_members,
      activeTimers: teamResult.data.active_timers,
      members: teamResult.data.members.map(member => ({
        name: member.name,
        isOnline: member.is_online,
        currentTask: member.current_task?.title,
        timerRunning: member.current_task?.timer_running
      }))
    });
  }
  
  // Example 6: Get complete dashboard data
  const dashboardResult = await getDashboardData(workspaceId, {
    cache: true,
    timeout: 15000
  });
  
  if (dashboardResult.success && dashboardResult.data) {
    console.log('Complete Dashboard Data:', {
      statistics: dashboardResult.data.statistics,
      recentActivitiesCount: dashboardResult.data.recent_activities.length,
      recentTasksCount: dashboardResult.data.recent_tasks.length,
      heatmapYear: dashboardResult.data.heatmap_data.year,
      teamMembersCount: dashboardResult.data.team_activity.total_members
    });
  }
  
  // Example 7: Refresh specific dashboard section
  const refreshResult = await refreshDashboardSection(workspaceId, 'statistics', {
    force: true,
    timeout: 5000
  });
  
  if (refreshResult.success) {
    console.log('Statistics refreshed successfully');
  } else if (refreshResult.error) {
    console.error('Failed to refresh statistics:', refreshResult.error.message);
  }
  
  // Example 8: Using retry mechanism for unreliable operations
  const retryResult = await withRetry(
    () => getDashboardStatistics(workspaceId),
    3, // max retries
    2000, // retry delay
    true // exponential backoff
  );
  
  if (retryResult.success && retryResult.data) {
    console.log('Statistics loaded after retry:', retryResult.data.total_tasks);
  } else if (retryResult.error) {
    console.error('Failed after all retries:', retryResult.error.message);
  }
}

// 8. Error Handling Examples
export function handleDashboardErrors(result: ActionResult<any>) {
  if (!result.success && result.error) {
    switch (result.error.type) {
      case 'network':
        console.error('Network error - check connection');
        // Show offline indicator or retry button
        break;
        
      case 'validation':
        console.error('Validation error - check request parameters');
        // Show form validation errors
        break;
        
      case 'authorization':
        console.error('Authorization error - user needs to login');
        // Redirect to login page
        break;
        
      case 'server':
        console.error('Server error - try again later');
        // Show generic error message with retry option
        break;
        
      case 'timeout':
        console.error('Request timeout - operation took too long');
        // Show timeout message with retry option
        break;
        
      default:
        console.error('Unknown error:', result.error.message);
        // Show generic error message
        break;
    }
    
    // Check if error is recoverable
    if (result.error.recoverable) {
      console.log('Error is recoverable - showing retry option');
    }
  }
}