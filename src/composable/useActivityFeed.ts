import { ref, computed, watch, onUnmounted, getCurrentInstance } from 'vue';
import type { Ref, ComputedRef } from 'vue';
import store from '@/store';
import { getActivityFeed, withRetry } from '@/actions/tmgr/dashboard';
import { usePusher } from './usePusher';
import type {
  Activity,
  ActivityType,
  ActivityFilters,
  SortOption,
  ActivityFeedParams,
  ActionError,
  EventHandlers
} from '@/types/dashboard';

// Activity feed return interface
export interface UseActivityFeedReturn {
  // State
  activities: Ref<Activity[]>;
  filters: Ref<ActivityFilters>;
  sortOption: Ref<SortOption>;
  
  // Pagination
  currentPage: Ref<number>;
  totalPages: Ref<number>;
  totalItems: Ref<number>;
  hasMore: Ref<boolean>;
  perPage: Ref<number>;
  
  // Loading states
  loading: Ref<boolean>;
  loadingMore: Ref<boolean>;
  refreshing: Ref<boolean>;
  error: Ref<ActionError | null>;
  
  // Actions
  loadActivities: (reset?: boolean) => Promise<void>;
  loadMore: () => Promise<void>;
  refresh: () => Promise<void>;
  applyFilters: (newFilters: Partial<ActivityFilters>) => Promise<void>;
  applySorting: (newSort: SortOption) => Promise<void>;
  clearFilters: () => Promise<void>;
  clearError: () => void;
  
  // Real-time
  enableRealTime: (workspaceId: number) => void;
  disableRealTime: () => void;
  
  // Computed
  filteredActivities: ComputedRef<Activity[]>;
  hasFilters: ComputedRef<boolean>;
  isEmpty: ComputedRef<boolean>;
  canLoadMore: ComputedRef<boolean>;
  
  // Utility
  findActivityById: (id: number) => Activity | undefined;
  getActivityTypeCount: (type: ActivityType) => number;
  getActivitiesByUser: (userId: number) => Activity[];
  getActivitiesByDateRange: (from: string, to: string) => Activity[];
}

// Default filters
const createDefaultFilters = (): ActivityFilters => ({
  type: 'all',
  user_id: 'all',
  limit: 20
});

// Default sort option
const createDefaultSort = (): SortOption => ({
  field: 'created_at',
  direction: 'desc'
});

// This was removed as it's not used

/**
 * Activity feed composable for managing activity list state and operations
 * Provides filtering, sorting, pagination, and real-time updates
 */
export function useActivityFeed(workspaceId?: number | { value: number }): UseActivityFeedReturn {
  // Get workspace ID from context if not provided
  const getCurrentWorkspaceId = (): number => {
    // Handle both direct number and ref
    const wsId = typeof workspaceId === 'object' && 'value' in workspaceId 
      ? workspaceId.value 
      : workspaceId;
    
    if (wsId && wsId > 0) return wsId;
    
    // Try to get from store or route params
    if (store?.state?.workspace?.current?.id) {
      return store.state.workspace.current.id;
    }
    
    throw new Error('Workspace ID is required for activity feed operations');
  };

  // State
  const activities = ref<Activity[]>([]);
  const filters = ref<ActivityFilters>(createDefaultFilters());
  const sortOption = ref<SortOption>(createDefaultSort());
  
  // Pagination state
  const currentPage = ref(1);
  const totalPages = ref(1);
  const totalItems = ref(0);
  const perPage = ref(20);
  
  // Loading states
  const loading = ref(false);
  const loadingMore = ref(false);
  const refreshing = ref(false);
  const error = ref<ActionError | null>(null);
  
  // Real-time state
  const realTimeEnabled = ref(false);
  const pusherInstance = ref<ReturnType<typeof usePusher> | null>(null);
  
  // Retry configuration
  const retryConfig = {
    maxRetries: 3,
    retryDelay: 1000,
    exponentialBackoff: true
  };

  // Computed properties
  const hasMore = computed(() => currentPage.value < totalPages.value);
  
  const filteredActivities = computed(() => {
    let result = activities.value;
    
    // Apply client-side filtering for better UX
    if (filters.value.search) {
      const searchTerm = filters.value.search.toLowerCase();
      result = result.filter(activity => 
        activity.description.toLowerCase().includes(searchTerm) ||
        activity.user.name.toLowerCase().includes(searchTerm)
      );
    }
    
    return result;
  });
  
  const hasFilters = computed(() => {
    const f = filters.value;
    return f.type !== 'all' || 
           f.user_id !== 'all' || 
           !!f.search || 
           !!f.date_from || 
           !!f.date_to;
  });
  
  const isEmpty = computed(() => activities.value.length === 0);
  
  const canLoadMore = computed(() => {
    return hasMore.value && 
           !loading.value && 
           !loadingMore.value && 
           !refreshing.value;
  });

  // Helper function to handle API errors
  const handleError = (apiError: ActionError): void => {
    error.value = apiError;
    console.error('Activity feed operation failed:', apiError);
  };

  // Clear error state
  const clearError = (): void => {
    error.value = null;
  };

  // Build API parameters from current state
  const buildApiParams = (page = 1): ActivityFeedParams => ({
    page,
    per_page: perPage.value,
    filters: {
      ...filters.value,
      // Convert 'all' values to undefined for API
      type: filters.value.type === 'all' ? undefined : filters.value.type,
      user_id: filters.value.user_id === 'all' ? undefined : filters.value.user_id
    },
    sort: sortOption.value
  });

  /**
   * Load activities from API
   */
  const loadActivities = async (reset = false): Promise<void> => {
    if (reset) {
      currentPage.value = 1;
      activities.value = [];
    }
    
    loading.value = true;
    clearError();
    
    try {
      const wsId = getCurrentWorkspaceId();
      const params = buildApiParams(currentPage.value);
      
      const result = await withRetry(
        () => getActivityFeed(wsId, params, { cache: currentPage.value === 1 }),
        retryConfig.maxRetries,
        retryConfig.retryDelay,
        retryConfig.exponentialBackoff
      );

      if (result.success && result.data) {
        const { data: activitiesData, meta } = result.data;
        
        if (reset || currentPage.value === 1) {
          activities.value = activitiesData;
        } else {
          activities.value.push(...activitiesData);
        }
        
        // Update pagination info
        currentPage.value = meta.current_page;
        totalPages.value = meta.last_page;
        totalItems.value = meta.total;
        
      } else if (result.error) {
        handleError(result.error);
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to load activities';
      handleError({
        message: errorMessage,
        type: 'server',
        timestamp: new Date().toISOString(),
        recoverable: true
      });
    } finally {
      loading.value = false;
    }
  };

  /**
   * Load more activities (pagination)
   */
  const loadMore = async (): Promise<void> => {
    if (!canLoadMore.value) return;
    
    loadingMore.value = true;
    clearError();
    
    try {
      const nextPage = currentPage.value + 1;
      const wsId = getCurrentWorkspaceId();
      const params = buildApiParams(nextPage);
      
      const result = await withRetry(
        () => getActivityFeed(wsId, params),
        retryConfig.maxRetries,
        retryConfig.retryDelay,
        retryConfig.exponentialBackoff
      );

      if (result.success && result.data) {
        const { data: activitiesData, meta } = result.data;
        
        activities.value.push(...activitiesData);
        currentPage.value = meta.current_page;
        totalPages.value = meta.last_page;
        totalItems.value = meta.total;
        
      } else if (result.error) {
        handleError(result.error);
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to load more activities';
      handleError({
        message: errorMessage,
        type: 'server',
        timestamp: new Date().toISOString(),
        recoverable: true
      });
    } finally {
      loadingMore.value = false;
    }
  };

  /**
   * Refresh activities
   */
  const refresh = async (): Promise<void> => {
    refreshing.value = true;
    currentPage.value = 1;
    
    try {
      await loadActivities(true);
    } finally {
      refreshing.value = false;
    }
  };

  /**
   * Apply new filters
   */
  const applyFilters = async (newFilters: Partial<ActivityFilters>): Promise<void> => {
    // Update filters
    filters.value = { ...filters.value, ...newFilters };
    
    // Reset pagination and reload
    currentPage.value = 1;
    await loadActivities(true);
  };

  /**
   * Apply new sorting
   */
  const applySorting = async (newSort: SortOption): Promise<void> => {
    sortOption.value = newSort;
    
    // Reset pagination and reload
    currentPage.value = 1;
    await loadActivities(true);
  };

  /**
   * Clear all filters
   */
  const clearFilters = async (): Promise<void> => {
    filters.value = createDefaultFilters();
    sortOption.value = createDefaultSort();
    
    // Reset pagination and reload
    currentPage.value = 1;
    await loadActivities(true);
  };

  /**
   * Enable real-time updates
   */
  const enableRealTime = (wsId: number): void => {
    if (realTimeEnabled.value) return;
    
    try {
      pusherInstance.value = usePusher();
      
      const eventHandlers: EventHandlers = {
        onActivityCreated: (activity: Activity) => {
          // Add new activity to the beginning of the list
          activities.value.unshift(activity);
          
          // Keep list size manageable
          if (activities.value.length > 100) {
            activities.value = activities.value.slice(0, 100);
          }
          
          // Update total count
          totalItems.value += 1;
        },
        
        onError: (error: ActionError) => {
          console.error('Real-time activity feed error:', error);
        },
        
        onReconnect: () => {
          console.log('Activity feed real-time reconnected');
          // Optionally refresh data on reconnect
          refresh();
        }
      };
      
      if (pusherInstance.value) {
        pusherInstance.value.subscribeToWorkspace(wsId, eventHandlers);
      }
      realTimeEnabled.value = true;
      
      console.log('Activity feed real-time updates enabled');
    } catch (err) {
      console.error('Failed to enable real-time updates:', err);
    }
  };

  /**
   * Disable real-time updates
   */
  const disableRealTime = (): void => {
    if (!realTimeEnabled.value || !pusherInstance.value) return;
    
    try {
      const wsId = getCurrentWorkspaceId();
      if (pusherInstance.value) {
        pusherInstance.value.unsubscribeFromWorkspace(wsId);
      }
      pusherInstance.value = null;
      realTimeEnabled.value = false;
      
      console.log('Activity feed real-time updates disabled');
    } catch (err) {
      console.error('Failed to disable real-time updates:', err);
    }
  };

  /**
   * Find activity by ID
   */
  const findActivityById = (id: number): Activity | undefined => {
    return activities.value.find(activity => activity.id === id);
  };

  /**
   * Get count of activities by type
   */
  const getActivityTypeCount = (type: ActivityType): number => {
    return activities.value.filter(activity => activity.type === type).length;
  };

  /**
   * Get activities by user
   */
  const getActivitiesByUser = (userId: number): Activity[] => {
    return activities.value.filter(activity => activity.user.id === userId);
  };

  /**
   * Get activities by date range
   */
  const getActivitiesByDateRange = (from: string, to: string): Activity[] => {
    const fromDate = new Date(from);
    const toDate = new Date(to);
    
    return activities.value.filter(activity => {
      const activityDate = new Date(activity.created_at);
      return activityDate >= fromDate && activityDate <= toDate;
    });
  };

  // Internal utility functions (not exposed in return interface)
  // These are used internally by the real-time event handlers

  // Watch for filter changes to auto-reload
  watch(
    () => [filters.value.type, filters.value.user_id],
    () => {
      // Debounce the reload to avoid too many API calls
      clearTimeout((window as any).activityFilterTimeout);
      (window as any).activityFilterTimeout = setTimeout(() => {
        loadActivities(true);
      }, 300);
    },
    { deep: true }
  );

  // Cleanup on unmount (only if called within component setup)
  if (getCurrentInstance()) {
    onUnmounted(() => {
      disableRealTime();
      clearTimeout((window as any).activityFilterTimeout);
    });
  }

  return {
    // State
    activities: activities as Ref<Activity[]>,
    filters: filters as Ref<ActivityFilters>,
    sortOption: sortOption as Ref<SortOption>,
    
    // Pagination
    currentPage: currentPage as Ref<number>,
    totalPages: totalPages as Ref<number>,
    totalItems: totalItems as Ref<number>,
    hasMore: hasMore as Ref<boolean>,
    perPage: perPage as Ref<number>,
    
    // Loading states
    loading: loading as Ref<boolean>,
    loadingMore: loadingMore as Ref<boolean>,
    refreshing: refreshing as Ref<boolean>,
    error: error as Ref<ActionError | null>,
    
    // Actions
    loadActivities,
    loadMore,
    refresh,
    applyFilters,
    applySorting,
    clearFilters,
    clearError,
    
    // Real-time
    enableRealTime,
    disableRealTime,
    
    // Computed
    filteredActivities: filteredActivities as ComputedRef<Activity[]>,
    hasFilters: hasFilters as ComputedRef<boolean>,
    isEmpty: isEmpty as ComputedRef<boolean>,
    canLoadMore: canLoadMore as ComputedRef<boolean>,
    
    // Utility
    findActivityById,
    getActivityTypeCount,
    getActivitiesByUser,
    getActivitiesByDateRange,
    

  };
}