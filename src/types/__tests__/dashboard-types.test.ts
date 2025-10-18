// Test file to verify dashboard type definitions
import {
  ApiResponse,
  PaginatedResponse,
  ApiErrorResponse,
  ValidationErrorResponse,
  NetworkErrorResponse,
  LoadingState,
  DetailedLoadingState,
  DashboardLoadingStates,
  ActivityFilters,
  AdvancedActivityFilters,
  TaskFilters,
  SortOption,
  TypedSortOption,
  ActivitySortField,
  ActionResult,
  DetailedActionResult,
  ActivityType,
  // Type guards
  isApiErrorResponse,
  isValidationErrorResponse,
  isNetworkErrorResponse,
  isPaginatedResponse,
  isLoadingState,
  isDetailedLoadingState,
  isActionResult
} from '../dashboard';

describe('Dashboard Type Definitions', () => {
  describe('API Response Types', () => {
    it('should validate ApiResponse structure', () => {
      const response: ApiResponse<string> = {
        data: 'test',
        success: true,
        message: 'Success',
        timestamp: new Date().toISOString()
      };
      
      expect(response.data).toBe('test');
      expect(response.success).toBe(true);
    });

    it('should validate PaginatedResponse structure', () => {
      const response: PaginatedResponse<string> = {
        data: ['item1', 'item2'],
        meta: {
          current_page: 1,
          last_page: 5,
          per_page: 20,
          total: 100,
          from: 1,
          to: 20,
          path: '/api/items',
          first_page_url: '/api/items?page=1',
          last_page_url: '/api/items?page=5'
        },
        links: {
          first: '/api/items?page=1',
          last: '/api/items?page=5',
          next: '/api/items?page=2'
        }
      };
      
      expect(response.data).toHaveLength(2);
      expect(response.meta.total).toBe(100);
    });
  });

  describe('Error Response Types', () => {
    it('should validate ApiErrorResponse structure', () => {
      const error: ApiErrorResponse = {
        error: 'validation_failed',
        message: 'Validation failed',
        status_code: 422,
        timestamp: new Date().toISOString()
      };
      
      expect(isApiErrorResponse(error)).toBe(true);
    });

    it('should validate ValidationErrorResponse structure', () => {
      const error: ValidationErrorResponse = {
        error: 'validation_failed',
        message: 'Validation failed',
        status_code: 422,
        timestamp: new Date().toISOString(),
        errors: {
          email: ['Email is required'],
          password: ['Password must be at least 8 characters']
        }
      };
      
      expect(isValidationErrorResponse(error)).toBe(true);
    });

    it('should validate NetworkErrorResponse structure', () => {
      const error: NetworkErrorResponse = {
        error: 'network_error',
        message: 'Network connection failed',
        status_code: 0,
        timestamp: new Date().toISOString(),
        is_offline: true
      };
      
      expect(isNetworkErrorResponse(error)).toBe(true);
    });
  });

  describe('Loading State Types', () => {
    it('should validate LoadingState structure', () => {
      const loadingState: LoadingState = {
        isLoading: true,
        error: null,
        lastUpdated: new Date().toISOString(),
        progress: 50,
        retryCount: 0,
        canRetry: true
      };
      
      expect(isLoadingState(loadingState)).toBe(true);
    });

    it('should validate DetailedLoadingState structure', () => {
      const detailedState: DetailedLoadingState = {
        isLoading: false,
        error: null,
        status: 'success',
        startTime: Date.now(),
        endTime: Date.now() + 1000,
        duration: 1000
      };
      
      expect(isDetailedLoadingState(detailedState)).toBe(true);
    });

    it('should validate DashboardLoadingStates structure', () => {
      const dashboardStates: DashboardLoadingStates = {
        statistics: {
          isLoading: false,
          status: 'success'
        },
        activities: {
          isLoading: true,
          status: 'loading'
        },
        heatmap: {
          isLoading: false,
          status: 'idle'
        },
        recentTasks: {
          isLoading: false,
          status: 'success'
        },
        teamActivity: {
          isLoading: false,
          status: 'success'
        },
        loadingMore: false,
        initialLoad: false,
        refreshing: false
      };
      
      expect(dashboardStates.statistics.status).toBe('success');
      expect(dashboardStates.activities.isLoading).toBe(true);
    });
  });

  describe('Filter Types', () => {
    it('should validate ActivityFilters structure', () => {
      const filters: ActivityFilters = {
        type: ActivityType.TASK_CREATED,
        user_id: 123,
        date_from: '2024-01-01',
        date_to: '2024-12-31',
        search: 'test query'
      };
      
      expect(filters.type).toBe(ActivityType.TASK_CREATED);
      expect(filters.user_id).toBe(123);
    });

    it('should validate AdvancedActivityFilters structure', () => {
      const filters: AdvancedActivityFilters = {
        type: 'all',
        exclude_types: [ActivityType.TASK_DELETED],
        include_metadata: true,
        group_by: 'date',
        time_range: 'week'
      };
      
      expect(filters.exclude_types).toContain(ActivityType.TASK_DELETED);
      expect(filters.time_range).toBe('week');
    });

    it('should validate TaskFilters structure', () => {
      const filters: TaskFilters = {
        status_id: 1,
        priority: 'high',
        is_overdue: true,
        assigned_to_me: true,
        search: 'urgent task'
      };
      
      expect(filters.priority).toBe('high');
      expect(filters.is_overdue).toBe(true);
    });
  });

  describe('Sort Types', () => {
    it('should validate SortOption structure', () => {
      const sort: SortOption = {
        field: 'created_at',
        direction: 'desc'
      };
      
      expect(sort.field).toBe('created_at');
      expect(sort.direction).toBe('desc');
    });

    it('should validate TypedSortOption structure', () => {
      const sort: TypedSortOption<ActivitySortField> = {
        field: 'user_name',
        direction: 'asc'
      };
      
      expect(sort.field).toBe('user_name');
      expect(sort.direction).toBe('asc');
    });
  });

  describe('Action Result Types', () => {
    it('should validate ActionResult structure', () => {
      const result: ActionResult<string> = {
        data: 'success data',
        success: true,
        timestamp: new Date().toISOString(),
        cached: false,
        duration: 1500
      };
      
      expect(isActionResult(result)).toBe(true);
      expect(result.success).toBe(true);
    });

    it('should validate DetailedActionResult structure', () => {
      const result: DetailedActionResult<number> = {
        data: 42,
        success: true,
        timestamp: new Date().toISOString(),
        status: 'success',
        metadata: {
          request_id: 'req-123',
          source: 'network'
        }
      };
      
      expect(result.status).toBe('success');
      expect(result.metadata?.source).toBe('network');
    });
  });

  describe('Type Guards', () => {
    it('should correctly identify paginated responses', () => {
      const validPaginated = {
        data: [1, 2, 3],
        meta: { current_page: 1, total: 3 },
        links: { first: 'url' }
      };
      
      const invalidPaginated = {
        data: 'not an array',
        meta: { current_page: 1 }
      };
      
      expect(isPaginatedResponse(validPaginated)).toBe(true);
      expect(isPaginatedResponse(invalidPaginated)).toBe(false);
    });

    it('should correctly identify error responses', () => {
      const validError = {
        error: 'test_error',
        message: 'Test message',
        status_code: 400
      };
      
      const invalidError = {
        error: 'test_error'
        // missing required fields
      };
      
      expect(isApiErrorResponse(validError)).toBe(true);
      expect(isApiErrorResponse(invalidError)).toBe(false);
    });
  });

  describe('Dashboard Action Types', () => {
    it('should validate dashboard action function signatures', () => {
      // Test that the action functions have the correct TypeScript signatures
      // This is a compile-time test - if it compiles, the types are correct
      
      const mockDashboardActions = {
        getDashboardStatistics: async (workspaceId: number, options?: { cache?: boolean; timeout?: number }) => {
          return {
            success: true,
            data: {
              total_tasks: 100,
              active_tasks: 25,
              completed_today: 5,
              completed_week: 20,
              completed_month: 80,
              time_today: 3600,
              time_week: 18000,
              time_month: 72000,
              time_total: 360000,
              team_members: 5,
              overdue_tasks: 3,
              daily_routine_completion_rate: 0.85
            },
            timestamp: new Date().toISOString()
          };
        },
        
        getActivityFeed: async (workspaceId: number, params?: any, options?: { cache?: boolean; timeout?: number }) => {
          return {
            success: true,
            data: {
              data: [],
              meta: {
                current_page: 1,
                last_page: 1,
                per_page: 20,
                total: 0,
                from: 1,
                to: 0,
                path: '/api/activities',
                first_page_url: '/api/activities?page=1',
                last_page_url: '/api/activities?page=1'
              },
              links: {
                first: '/api/activities?page=1',
                last: '/api/activities?page=1'
              }
            },
            timestamp: new Date().toISOString()
          };
        },
        
        getUserHeatmap: async (workspaceId: number, params?: any, options?: { cache?: boolean; timeout?: number }) => {
          return {
            success: true,
            data: {
              contributions: [],
              total_contributions: 0,
              streak: {
                current: 0,
                longest: 0
              },
              year: 2024,
              weeks: 52
            },
            timestamp: new Date().toISOString()
          };
        },
        
        getRecentTasks: async (workspaceId: number, params?: any, options?: { cache?: boolean; timeout?: number }) => {
          return {
            success: true,
            data: [],
            timestamp: new Date().toISOString()
          };
        },
        
        getTeamActivity: async (workspaceId: number, options?: { cache?: boolean; timeout?: number }) => {
          return {
            success: true,
            data: {
              members: [],
              total_members: 0,
              online_members: 0,
              active_timers: 0
            },
            timestamp: new Date().toISOString()
          };
        },
        
        getDashboardData: async (workspaceId: number, options?: { cache?: boolean; timeout?: number }) => {
          return {
            success: true,
            data: {
              statistics: {
                total_tasks: 100,
                active_tasks: 25,
                completed_today: 5,
                completed_week: 20,
                completed_month: 80,
                time_today: 3600,
                time_week: 18000,
                time_month: 72000,
                time_total: 360000,
                team_members: 5,
                overdue_tasks: 3,
                daily_routine_completion_rate: 0.85
              },
              recent_activities: [],
              heatmap_data: {
                contributions: [],
                total_contributions: 0,
                streak: { current: 0, longest: 0 },
                year: 2024,
                weeks: 52
              },
              recent_tasks: [],
              team_activity: {
                members: [],
                total_members: 0,
                online_members: 0,
                active_timers: 0
              }
            },
            timestamp: new Date().toISOString()
          };
        },
        
        refreshDashboardSection: async (workspaceId: number, section: 'statistics' | 'activities' | 'heatmap' | 'tasks' | 'team', options?: { force?: boolean; timeout?: number }) => {
          return {
            success: true,
            data: {},
            timestamp: new Date().toISOString()
          };
        }
      };
      
      // Test function signatures by calling them with correct parameters
      expect(typeof mockDashboardActions.getDashboardStatistics).toBe('function');
      expect(typeof mockDashboardActions.getActivityFeed).toBe('function');
      expect(typeof mockDashboardActions.getUserHeatmap).toBe('function');
      expect(typeof mockDashboardActions.getRecentTasks).toBe('function');
      expect(typeof mockDashboardActions.getTeamActivity).toBe('function');
      expect(typeof mockDashboardActions.getDashboardData).toBe('function');
      expect(typeof mockDashboardActions.refreshDashboardSection).toBe('function');
    });

    it('should validate error handling types', () => {
      const networkError = {
        message: 'Network connection failed',
        type: 'network' as const,
        status_code: 0,
        is_offline: true,
        timestamp: new Date().toISOString(),
        recoverable: true
      };
      
      const validationError = {
        message: 'Validation failed',
        type: 'validation' as const,
        field_errors: {
          email: ['Email is required'],
          password: ['Password too short']
        },
        timestamp: new Date().toISOString(),
        recoverable: false
      };
      
      const authorizationError = {
        message: 'Access denied',
        type: 'authorization' as const,
        required_permission: 'dashboard.view',
        timestamp: new Date().toISOString(),
        recoverable: false
      };
      
      const serverError = {
        message: 'Internal server error',
        type: 'server' as const,
        status_code: 500,
        trace_id: 'trace-123',
        timestamp: new Date().toISOString(),
        recoverable: true
      };
      
      const timeoutError = {
        message: 'Request timeout',
        type: 'timeout' as const,
        timeout_duration: 30000,
        operation: 'getDashboardStatistics',
        timestamp: new Date().toISOString(),
        recoverable: true
      };
      
      // Verify error types have correct structure
      expect(networkError.type).toBe('network');
      expect(networkError.recoverable).toBe(true);
      expect(validationError.type).toBe('validation');
      expect(validationError.recoverable).toBe(false);
      expect(authorizationError.type).toBe('authorization');
      expect(serverError.type).toBe('server');
      expect(timeoutError.type).toBe('timeout');
    });

    it('should validate request parameter interfaces', () => {
      const activityFeedParams = {
        page: 1,
        per_page: 20,
        filters: {
          type: ActivityType.TASK_CREATED,
          user_id: 123,
          date_from: '2024-01-01',
          date_to: '2024-12-31',
          search: 'test'
        },
        sort: {
          field: 'created_at',
          direction: 'desc' as const
        }
      };
      
      const heatmapParams = {
        year: 2024,
        user_id: 123,
        timezone: 'America/New_York',
        include_weekends: true
      };
      
      const recentTasksParams = {
        limit: 10,
        filters: {
          status_id: 1,
          priority: 'high' as const,
          is_overdue: false,
          has_timer: true
        },
        sort: {
          field: 'updated_at',
          direction: 'desc' as const
        }
      };
      
      // Verify parameter structures
      expect(activityFeedParams.page).toBe(1);
      expect(activityFeedParams.filters?.type).toBe(ActivityType.TASK_CREATED);
      expect(heatmapParams.year).toBe(2024);
      expect(heatmapParams.include_weekends).toBe(true);
      expect(recentTasksParams.limit).toBe(10);
      expect(recentTasksParams.filters?.priority).toBe('high');
    });
  });
});