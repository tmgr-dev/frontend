import { ref, computed, reactive, onUnmounted, getCurrentInstance } from 'vue';
import type { Ref, ComputedRef } from 'vue';
import store from '@/store';
import {
  getDashboardStatistics,
  getActivityFeed,
  getUserHeatmap,
  getRecentTasks,
  getTeamActivity,
  getDashboardData,
  refreshDashboardSection,
  withRetry
} from '@/actions/tmgr/dashboard';
import type {
  DashboardStatistics,
  Activity,
  HeatmapData,
  RecentTask,
  TeamMemberActivity,
  DashboardLoadingStates,
  DetailedLoadingState,
  ActionError,
  ActivityFeedParams,
  HeatmapParams,
  RecentTasksParams,
  UseDashboardReturn
} from '@/types/dashboard';

// Loading state factory functions
const createInitialState = (): DetailedLoadingState => ({
  isLoading: false,
  error: null,
  status: 'idle',
  lastUpdated: undefined,
  progress: 0,
  retryCount: 0,
  canRetry: true
});

const createLoadingState = (progress = 0): DetailedLoadingState => ({
  isLoading: true,
  error: null,
  status: 'loading',
  progress,
  retryCount: 0,
  canRetry: true,
  startTime: Date.now()
});

const createSuccessState = (duration?: number): DetailedLoadingState => ({
  isLoading: false,
  error: null,
  status: 'success',
  lastUpdated: new Date().toISOString(),
  progress: 100,
  retryCount: 0,
  canRetry: true,
  endTime: Date.now(),
  duration
});

const createErrorState = (error: string, canRetry = true): DetailedLoadingState => ({
  isLoading: false,
  error,
  status: 'error',
  progress: 0,
  canRetry,
  endTime: Date.now()
});

const createRetryingState = (retryCount: number): DetailedLoadingState => ({
  isLoading: true,
  error: null,
  status: 'retrying',
  progress: 0,
  retryCount,
  canRetry: true,
  startTime: Date.now()
});

/**
 * Dashboard composable for managing dashboard state and operations
 * Provides reactive state management for all dashboard data sections
 */
export function useDashboard(workspaceId?: number | Ref<number>): UseDashboardReturn {
  // Get workspace ID from current context if not provided
  const getCurrentWorkspaceId = (): number | null => {
    // Handle both direct number and ref
    const wsId = typeof workspaceId === 'object' && 'value' in workspaceId 
      ? workspaceId.value 
      : workspaceId;
    
    if (wsId && wsId > 0) return wsId;
    
    // Try to get from store or route params
    
    // Try different store structures
    if (store?.state?.workspace?.current?.id) {
      return store.state.workspace.current.id;
    }
    
    // Try workspaces array with current workspace setting
    if (store?.state?.workspaces?.length) {
      const currentWorkspaceId = store.state.user?.settings?.find(
        (setting: { key: string, value: any }) => setting.key === 'current_workspace'
      )?.value;
      
      // If we have a current workspace setting, validate it exists in user's workspaces
      if (currentWorkspaceId) {
        const currentWorkspace = store.state.workspaces.find(
          (workspace: { id: number, name: string, code: string }) => 
            Number(workspace.id) === Number(currentWorkspaceId)
        );
        
        if (currentWorkspace?.id) {
          return currentWorkspace.id;
        }
      }
      
      // Fallback to first workspace (but only if user has workspaces)
      if (store.state.workspaces[0]?.id) {
        return store.state.workspaces[0].id;
      }
    }
    
    // No valid workspace found
    return null;
  };

  // Reactive state for dashboard data
  const statistics = ref<DashboardStatistics | null>(null);
  const activities = ref<Activity[]>([]);
  const heatmapData = ref<HeatmapData | null>(null);
  const recentTasks = ref<RecentTask[]>([]);
  const teamActivity = ref<TeamMemberActivity | null>(null);

  // Loading states for each section
  const loadingStates = ref<DashboardLoadingStates>({
    statistics: createInitialState(),
    activities: createInitialState(),
    heatmap: createInitialState(),
    recentTasks: createInitialState(),
    teamActivity: createInitialState(),
    loadingMore: false,
    initialLoad: true,
    refreshing: false
  });

  // Global error state
  const error = ref<ActionError | null>(null);

  // Pagination state for activities
  const activitiesPage = ref(1);
  const activitiesHasMore = ref(true);
  const activitiesPerPage = ref(20);

  // Retry configuration
  const retryConfig = {
    maxRetries: 3,
    retryDelay: 1000,
    exponentialBackoff: true
  };

  // Computed properties
  const hasError = computed(() => error.value !== null);
  
  const isAnyLoading = computed(() => {
    const states = loadingStates.value;
    return states.statistics.isLoading ||
           states.activities.isLoading ||
           states.heatmap.isLoading ||
           states.recentTasks.isLoading ||
           states.teamActivity.isLoading ||
           states.loadingMore ||
           states.refreshing;
  });

  const canLoadMore = computed(() => {
    return activitiesHasMore.value && 
           !loadingStates.value.loadingMore && 
           !loadingStates.value.activities.isLoading;
  });

  const isInitialLoad = computed(() => loadingStates.value.initialLoad);

  // Helper function to update loading state
  const updateLoadingState = (
    section: keyof Omit<DashboardLoadingStates, 'loadingMore' | 'initialLoad' | 'refreshing'>,
    state: DetailedLoadingState
  ) => {
    loadingStates.value[section] = { ...state };
  };

  // Helper function to handle API errors
  const handleError = (apiError: ActionError, section?: string): void => {
    error.value = apiError;
    console.error(`Dashboard ${section || 'operation'} failed:`, apiError);
  };

  // Clear error state
  const clearError = (): void => {
    error.value = null;
  };

  /**
   * Load dashboard statistics
   */
  const loadStatistics = async (): Promise<void> => {
    const startTime = Date.now();
    updateLoadingState('statistics', createLoadingState(0));
    
    try {
      const wsId = getCurrentWorkspaceId();
      if (!wsId) {
        throw new Error('No valid workspace found. Please ensure you have access to at least one workspace and try refreshing the page.');
      }
      console.log('Loading statistics for workspace ID:', wsId);
      
      const result = await withRetry(
        () => getDashboardStatistics(wsId, { cache: true }),
        retryConfig.maxRetries,
        retryConfig.retryDelay,
        retryConfig.exponentialBackoff
      );

      if (result.success && result.data) {
        statistics.value = result.data;
        const duration = Date.now() - startTime;
        updateLoadingState('statistics', createSuccessState(duration));
      } else if (result.error) {
        updateLoadingState('statistics', createErrorState(result.error.message));
        handleError(result.error, 'statistics');
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to load statistics';
      updateLoadingState('statistics', createErrorState(errorMessage));
      handleError({
        message: errorMessage,
        type: 'server',
        timestamp: new Date().toISOString(),
        recoverable: true
      }, 'statistics');
    }
  };

  /**
   * Load activity feed
   */
  const loadActivities = async (page = 1, append = false): Promise<void> => {
    const startTime = Date.now();
    
    if (append) {
      loadingStates.value.loadingMore = true;
    } else {
      updateLoadingState('activities', createLoadingState(0));
    }
    
    try {
      const wsId = getCurrentWorkspaceId();
      if (!wsId) {
        throw new Error('No valid workspace found. Please ensure you have access to at least one workspace and try refreshing the page.');
      }
      
      const params: ActivityFeedParams = {
        page,
        per_page: activitiesPerPage.value,
        sort: { field: 'created_at', direction: 'desc' }
      };

      const result = await withRetry(
        () => getActivityFeed(wsId, params, { cache: page === 1 }),
        retryConfig.maxRetries,
        retryConfig.retryDelay,
        retryConfig.exponentialBackoff
      );

      if (result.success && result.data) {
        const { data: activitiesData, meta } = result.data;
        
        if (append) {
          activities.value.push(...activitiesData);
        } else {
          activities.value = activitiesData;
        }
        
        activitiesPage.value = meta.current_page;
        activitiesHasMore.value = meta.current_page < meta.last_page;
        
        const duration = Date.now() - startTime;
        
        if (append) {
          loadingStates.value.loadingMore = false;
        } else {
          updateLoadingState('activities', createSuccessState(duration));
        }
      } else if (result.error) {
        if (append) {
          loadingStates.value.loadingMore = false;
        } else {
          updateLoadingState('activities', createErrorState(result.error.message));
        }
        handleError(result.error, 'activities');
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to load activities';
      
      if (append) {
        loadingStates.value.loadingMore = false;
      } else {
        updateLoadingState('activities', createErrorState(errorMessage));
      }
      
      handleError({
        message: errorMessage,
        type: 'server',
        timestamp: new Date().toISOString(),
        recoverable: true
      }, 'activities');
    }
  };

  /**
   * Load heatmap data
   */
  const loadHeatmap = async (params?: HeatmapParams): Promise<void> => {
    const startTime = Date.now();
    updateLoadingState('heatmap', createLoadingState(0));
    
    try {
      const wsId = getCurrentWorkspaceId();
      if (!wsId) {
        throw new Error('No valid workspace found. Please ensure you have access to at least one workspace and try refreshing the page.');
      }
      console.log('Loading heatmap for workspace ID:', wsId);
      
      const heatmapParams: HeatmapParams = {
        year: new Date().getFullYear(),
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        include_weekends: true,
        ...params
      };

      const result = await withRetry(
        () => getUserHeatmap(wsId, heatmapParams, { cache: true }),
        retryConfig.maxRetries,
        retryConfig.retryDelay,
        retryConfig.exponentialBackoff
      );

      if (result.success && result.data) {
        heatmapData.value = result.data;
        const duration = Date.now() - startTime;
        updateLoadingState('heatmap', createSuccessState(duration));
      } else if (result.error) {
        updateLoadingState('heatmap', createErrorState(result.error.message));
        handleError(result.error, 'heatmap');
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to load heatmap';
      updateLoadingState('heatmap', createErrorState(errorMessage));
      handleError({
        message: errorMessage,
        type: 'server',
        timestamp: new Date().toISOString(),
        recoverable: true
      }, 'heatmap');
    }
  };

  /**
   * Load recent tasks
   */
  const loadRecentTasks = async (params?: RecentTasksParams): Promise<void> => {
    const startTime = Date.now();
    updateLoadingState('recentTasks', createLoadingState(0));
    
    try {
      const wsId = getCurrentWorkspaceId();
      if (!wsId) {
        throw new Error('No valid workspace found. Please ensure you have access to at least one workspace and try refreshing the page.');
      }
      
      const taskParams: RecentTasksParams = {
        limit: 10,
        sort: { field: 'updated_at', direction: 'desc' },
        ...params
      };

      const result = await withRetry(
        () => getRecentTasks(wsId, taskParams, { cache: true }),
        retryConfig.maxRetries,
        retryConfig.retryDelay,
        retryConfig.exponentialBackoff
      );

      if (result.success && result.data) {
        recentTasks.value = result.data;
        const duration = Date.now() - startTime;
        updateLoadingState('recentTasks', createSuccessState(duration));
      } else if (result.error) {
        updateLoadingState('recentTasks', createErrorState(result.error.message));
        handleError(result.error, 'recent tasks');
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to load recent tasks';
      updateLoadingState('recentTasks', createErrorState(errorMessage));
      handleError({
        message: errorMessage,
        type: 'server',
        timestamp: new Date().toISOString(),
        recoverable: true
      }, 'recent tasks');
    }
  };

  /**
   * Load team activity
   */
  const loadTeamActivity = async (): Promise<void> => {
    const startTime = Date.now();
    updateLoadingState('teamActivity', createLoadingState(0));
    
    try {
      const wsId = getCurrentWorkspaceId();
      if (!wsId) {
        throw new Error('No valid workspace found. Please ensure you have access to at least one workspace and try refreshing the page.');
      }

      const result = await withRetry(
        () => getTeamActivity(wsId, { cache: true }),
        retryConfig.maxRetries,
        retryConfig.retryDelay,
        retryConfig.exponentialBackoff
      );

      if (result.success && result.data) {
        teamActivity.value = result.data;
        const duration = Date.now() - startTime;
        updateLoadingState('teamActivity', createSuccessState(duration));
      } else if (result.error) {
        updateLoadingState('teamActivity', createErrorState(result.error.message));
        handleError(result.error, 'team activity');
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to load team activity';
      updateLoadingState('teamActivity', createErrorState(errorMessage));
      handleError({
        message: errorMessage,
        type: 'server',
        timestamp: new Date().toISOString(),
        recoverable: true
      }, 'team activity');
    }
  };

  /**
   * Load all dashboard data
   */
  const loadDashboard = async (): Promise<void> => {
    clearError();
    loadingStates.value.initialLoad = true;
    
    try {
      // Load all sections in parallel for better performance
      await Promise.allSettled([
        loadStatistics(),
        loadActivities(1, false),
        loadHeatmap(),
        loadRecentTasks(),
        loadTeamActivity()
      ]);
    } finally {
      loadingStates.value.initialLoad = false;
    }
  };

  /**
   * Load more activities (pagination)
   */
  const loadMoreActivities = async (): Promise<void> => {
    if (!canLoadMore.value) return;
    
    const nextPage = activitiesPage.value + 1;
    await loadActivities(nextPage, true);
  };

  /**
   * Refresh specific dashboard section
   */
  const refreshSection = async (
    section: keyof Omit<DashboardLoadingStates, 'loadingMore' | 'initialLoad' | 'refreshing'>
  ): Promise<void> => {
    loadingStates.value.refreshing = true;
    
    try {
      switch (section) {
        case 'statistics':
          await loadStatistics();
          break;
        case 'activities':
          activitiesPage.value = 1;
          activitiesHasMore.value = true;
          await loadActivities(1, false);
          break;
        case 'heatmap':
          await loadHeatmap();
          break;
        case 'recentTasks':
          await loadRecentTasks();
          break;
        case 'teamActivity':
          await loadTeamActivity();
          break;
      }
    } finally {
      loadingStates.value.refreshing = false;
    }
  };

  /**
   * Refresh all dashboard data
   */
  const refreshDashboard = async (): Promise<void> => {
    loadingStates.value.refreshing = true;
    clearError();
    
    try {
      // Reset pagination for activities
      activitiesPage.value = 1;
      activitiesHasMore.value = true;
      
      // Refresh all sections
      await Promise.allSettled([
        loadStatistics(),
        loadActivities(1, false),
        loadHeatmap(),
        loadRecentTasks(),
        loadTeamActivity()
      ]);
    } finally {
      loadingStates.value.refreshing = false;
    }
  };

  /**
   * Add new activity to the feed (for real-time updates)
   */
  const addActivity = (activity: Activity): void => {
    activities.value.unshift(activity);
    
    // Keep only the latest activities to prevent memory issues
    if (activities.value.length > 100) {
      activities.value = activities.value.slice(0, 100);
    }
  };

  /**
   * Update statistics (for real-time updates)
   */
  const updateStatistics = (newStats: Partial<DashboardStatistics>): void => {
    if (statistics.value) {
      statistics.value = { ...statistics.value, ...newStats };
    }
  };

  /**
   * Update team member status (for real-time updates)
   */
  const updateTeamMemberStatus = (memberId: number, updates: Partial<TeamMemberActivity>): void => {
    if (teamActivity.value) {
      // Update team activity data
      teamActivity.value = { ...teamActivity.value, ...updates };
    }
  };

  // Cleanup function
  const cleanup = (): void => {
    // Clear all reactive state
    statistics.value = null;
    activities.value = [];
    heatmapData.value = null;
    recentTasks.value = [];
    teamActivity.value = null;
    error.value = null;
    
    // Reset loading states
    loadingStates.value = {
      statistics: createInitialState(),
      activities: createInitialState(),
      heatmap: createInitialState(),
      recentTasks: createInitialState(),
      teamActivity: createInitialState(),
      loadingMore: false,
      initialLoad: true,
      refreshing: false
    };
  };

  // Auto-cleanup on unmount (only if called within component setup)
  if (getCurrentInstance()) {
    onUnmounted(() => {
      cleanup();
    });
  }

  return {
    // State
    statistics: statistics as Ref<DashboardStatistics | null>,
    activities: activities as Ref<Activity[]>,
    heatmapData: heatmapData as Ref<HeatmapData | null>,
    recentTasks: recentTasks as Ref<RecentTask[]>,
    teamActivity: teamActivity as Ref<TeamMemberActivity | null>,
    
    // Loading states
    loadingStates: loadingStates as Ref<DashboardLoadingStates>,
    
    // Error states
    error: error as Ref<ActionError | null>,
    
    // Actions
    loadDashboard,
    loadMoreActivities,
    loadHeatmap,
    refreshSection,
    refreshDashboard,
    clearError,
    
    // Real-time update methods
    addActivity,
    updateStatistics,
    updateTeamMemberStatus,
    
    // Computed
    hasError: hasError as ComputedRef<boolean>,
    isAnyLoading: isAnyLoading as ComputedRef<boolean>,
    canLoadMore: canLoadMore as ComputedRef<boolean>,
    isInitialLoad: isInitialLoad as ComputedRef<boolean>,
    
    // Utility
    cleanup
  };
}