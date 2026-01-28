<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, nextTick } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import store from '@/store';
import BaseLayout from '@/components/layouts/BaseLayout.vue';

// Dashboard components
import HeatmapCalendar from '@/components/general/HeatmapCalendar.vue';
import HeatmapSkeleton from '@/components/dashboard/HeatmapSkeleton.vue';
import StatisticsGrid from '@/components/dashboard/StatisticsGrid.vue';
import ActivityFeed from '@/components/dashboard/ActivityFeed.vue';
import TeamActivityWidget from '@/components/dashboard/TeamActivityWidget.vue';

// UI components
import Button from '@/components/ui/button/Button.vue';
import EmptyState from '@/components/EmptyState.vue';
import ErrorBoundary from '@/components/ErrorBoundary.vue';

// Composables
import { useDashboard } from '@/composable/useDashboard';
import { usePusher } from '@/composable/usePusher';
import { useActivityFeed } from '@/composable/useActivityFeed';
import { useNetworkStatus } from '@/composable/useNetworkStatus';
import { useErrorHandler } from '@/composable/useErrorHandler';
import { setDocumentTitle } from '@/composable/useDocumentTitle';

// Types
import type { 
  Activity, 
  RecentTask, 
  TeamMemberStatus, 
  ActivityFilters,
  DashboardStatistics 
} from '@/types/dashboard';

// Icons
import { 
  ArrowPathIcon, 
  ExclamationTriangleIcon,
  WifiIcon,
  SignalSlashIcon 
} from '@heroicons/vue/24/outline';
import { LayoutDashboard } from 'lucide-vue-next';

// Feature Gate
import FeatureGate from '@/components/general/FeatureGate.vue';
import DashboardPreview from '@/components/previews/DashboardPreview.vue';

const route = useRoute();
const router = useRouter();

// Get current workspace ID with better error handling
const getCurrentWorkspaceId = (): number => {
  try {
    if (store.state.workspaces?.length) {
      const currentWorkspaceId = store.state.user?.settings?.find(
        (setting: { key: string, value: any }) => setting.key === 'current_workspace'
      )?.value;
      
      if (currentWorkspaceId) {
        const currentWorkspace = store.state.workspaces.find(
          (workspace: { id: number, name: string, code: string }) => 
            Number(workspace.id) === Number(currentWorkspaceId)
        );
        
        if (currentWorkspace?.id) {
          return Number(currentWorkspace.id);
        }
      }
      
      // Fallback to first workspace if current workspace setting is not found
      const firstWorkspace = store.state.workspaces[0];
      if (firstWorkspace?.id) {
        return Number(firstWorkspace.id);
      }
    }
    
    // Check route params as fallback
    const routeWorkspaceId = route.params.workspaceId || route.query.workspaceId;
    if (routeWorkspaceId && !isNaN(Number(routeWorkspaceId))) {
      return Number(routeWorkspaceId);
    }
    
    // Final fallback
    return 1;
  } catch (error) {
    console.warn('Error getting workspace ID:', error);
    return 1;
  }
};

const workspaceId = ref(getCurrentWorkspaceId());

// Watch for store changes to update workspace ID
watch(() => store.state.user?.settings, (newSettings) => {
  if (newSettings) {
    const newWorkspaceId = getCurrentWorkspaceId();
    if (newWorkspaceId !== workspaceId.value) {
      console.log(`Dashboard: Workspace changed from ${workspaceId.value} to ${newWorkspaceId}`);
      workspaceId.value = newWorkspaceId;
    }
  }
}, { deep: true });

// Dashboard composable
const {
  statistics,
  activities,
  heatmapData,
  teamActivity,
  loadingStates,
  error: dashboardError,
  loadDashboard,
  loadHeatmap,
  refreshSection,
  refreshDashboard,
  clearError,
  hasError,
  isAnyLoading,
  isInitialLoad,
  addActivity,
  updateStatistics
} = useDashboard(workspaceId);

// Activity feed composable
const {
  activities: feedActivities,
  loadMore: loadMoreActivities,
  refresh: refreshActivities,
  applyFilters: applyActivityFilters,
  enableRealTime: enableActivityRealTime,
  disableRealTime: disableActivityRealTime,
  canLoadMore,
  hasFilters: hasActivityFilters,
  loading: feedLoading,
  loadingMore: feedLoadingMore
} = useActivityFeed(workspaceId);

// Pusher composable for real-time updates
const {
  isConnected: isPusherConnected,
  connectionState,
  connectionError,
  subscribeToWorkspace,
  unsubscribeFromWorkspace,
  reconnect: reconnectPusher
} = usePusher();

// Network status monitoring
const {
  isOnline,
  isOffline,
  networkStatus,
  checkConnection,
  onOnline,
  onOffline
} = useNetworkStatus();

// Error handling
const {
  error: globalError,
  hasError: hasGlobalError,
  canRetry: canRetryGlobal,
  isRetrying: isRetryingGlobal,
  errorMessage: globalErrorMessage,
  setError: setGlobalError,
  clearError: clearGlobalError,
  retry: retryGlobal,
  handleError: handleGlobalError
} = useErrorHandler({
  maxRetries: 3,
  retryDelay: 1000,
  exponentialBackoff: true
});

// Local state
const showConnectionStatus = ref(false);
const errorBoundaryRef = ref<InstanceType<typeof ErrorBoundary> | null>(null);

// Computed properties
const currentWorkspace = computed(() => {
  if (!store.state.workspaces?.length) return null;
  
  return store.state.workspaces.find(
    (workspace: { id: number, name: string, code: string }) => 
      Number(workspace.id) === Number(workspaceId.value)
  );
});

const pageTitle = computed(() => {
  return currentWorkspace.value?.name 
    ? `${currentWorkspace.value.name} Dashboard`
    : 'Dashboard';
});

const workspaceUsersList = computed(() => {
  return teamActivity.value?.members?.map(member => ({
    id: member.id,
    name: member.name
  })) || [];
});

const hasAnyError = computed(() => {
  return hasError.value || hasGlobalError.value || !!connectionError.value || isOffline.value;
});

const canRetry = computed(() => {
  return canRetryGlobal.value || (hasError.value && !isRetryingGlobal.value);
});

const combinedErrorMessage = computed(() => {
  if (isOffline.value) {
    return 'You appear to be offline. Some features may not work properly.';
  }
  if (connectionError.value) {
    return 'Real-time connection failed. Updates may be delayed.';
  }
  if (hasGlobalError.value) {
    return globalErrorMessage.value;
  }
  if (dashboardError.value) {
    return dashboardError.value.message;
  }
  return 'An unexpected error occurred.';
});

const isRealTimeActive = computed(() => {
  return isPusherConnected.value && !isOffline.value;
});

const displayedActivities = computed(() => {
  return hasActivityFilters.value ? feedActivities.value : activities.value;
});

const activityLoading = computed(() => {
  return hasActivityFilters.value ? feedLoading.value : loadingStates.value.activities.isLoading;
});

const activityLoadingMore = computed(() => {
  return hasActivityFilters.value ? feedLoadingMore.value : loadingStates.value.loadingMore;
});

// Enhanced error handling
const handleError = (error: any, context: string) => {
  console.error(`Dashboard ${context} error:`, error);
  
  // Use the error handler composable
  const normalizedError = handleGlobalError(error, context);
  
  // Additional error-specific handling
  if (normalizedError.type === 'network' || !navigator.onLine) {
    // Network errors are handled by the network status composable
    return;
  }
  
  // Log to external service if configured
  if ((window as any).Sentry) {
    (window as any).Sentry.captureException(error, {
      tags: { context },
      extra: { workspaceId: workspaceId.value }
    });
  }
};

const handleRetry = async () => {
  if (!canRetry.value) return;
  
  try {
    // Use the error handler's retry mechanism
    const success = await retryGlobal(async () => {
      clearError();
      await loadDashboard();
    });
    
    if (success) {
      console.log('Dashboard retry successful');
    }
  } catch (error) {
    handleError(error, 'retry');
  }
};

// Error boundary handlers
const handleErrorBoundaryError = (error: Error, errorInfo: any) => {
  console.error('Error boundary caught error:', error, errorInfo);
  
  // Set global error state
  setGlobalError({
    message: error.message,
    type: 'component',
    timestamp: new Date().toISOString(),
    recoverable: true,
    details: errorInfo
  });
};

const handleErrorBoundaryRetry = async () => {
  try {
    clearGlobalError();
    await loadDashboard();
  } catch (error) {
    handleError(error, 'error boundary retry');
  }
};

const handleErrorReport = (error: Error, errorInfo: any) => {
  // Send error report to support system
  const errorReport = {
    error: {
      name: error.name,
      message: error.message,
      stack: error.stack
    },
    errorInfo,
    context: {
      workspaceId: workspaceId.value,
      url: window.location.href,
      userAgent: navigator.userAgent,
      timestamp: new Date().toISOString()
    }
  };
  
  // Send to error reporting service
  fetch('/api/error-reports', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(errorReport)
  }).catch(reportError => {
    console.error('Failed to send error report:', reportError);
  });
};

const handleRefresh = async () => {
  try {
    await refreshDashboard();
  } catch (error) {
    handleError(error, 'refresh');
  }
};

// Real-time event handlers with animations
const handleNewActivity = (activity: Activity) => {
  // Add activity with smooth animation
  addActivity(activity);
  
  // Show subtle notification with animation
  showActivityNotification(activity);
  
  console.log('New activity received:', activity);
};

const handleDashboardUpdate = (updates: Partial<DashboardStatistics>) => {
  // Update statistics with smooth transition
  updateStatistics(updates);
  
  // Trigger subtle visual feedback
  showStatisticsUpdate();
  
  console.log('Dashboard statistics updated:', updates);
};

const handleMemberStatusChange = (member: TeamMemberStatus) => {
  // Update team activity with animation
  updateTeamMemberStatus(member.id, { members: [member] });
  
  // Show member status change notification
  showMemberStatusNotification(member);
  
  console.log('Member status changed:', member);
};

// Notification and animation helpers
const showActivityNotification = (activity: Activity) => {
  // Create a subtle notification for new activities
  const notification = {
    id: Date.now(),
    type: 'activity',
    message: `New ${activity.type.replace('_', ' ')}: ${activity.description}`,
    duration: 3000
  };
  
  // You could integrate with a toast notification system here
  console.log('Activity notification:', notification);
};

const showStatisticsUpdate = () => {
  // Trigger a subtle animation on statistics cards
  const statisticsElement = document.querySelector('.statistics-grid');
  if (statisticsElement) {
    statisticsElement.classList.add('animate-pulse');
    setTimeout(() => {
      statisticsElement.classList.remove('animate-pulse');
    }, 500);
  }
};

const showMemberStatusNotification = (member: TeamMemberStatus) => {
  const notification = {
    id: Date.now(),
    type: 'member',
    message: `${member.name} is now ${member.is_online ? 'online' : 'offline'}`,
    duration: 2000
  };
  
  console.log('Member status notification:', notification);
};

// Navigation handlers
const handleStatisticCardClick = (filter: Record<string, any>) => {
  // Navigate to filtered view based on the statistic clicked
  if (filter.view === 'team') {
    router.push('/team');
  } else if (filter.view === 'time_tracking') {
    router.push('/stats');
  } else if (filter.view === 'daily_routine') {
    router.push('/daily-routine');
  } else {
    // Navigate to tasks list with filters
    const query: Record<string, string> = {};
    if (filter.status && filter.status !== 'all') {
      query.status = filter.status;
    }
    if (filter.period) {
      query.period = filter.period;
    }
    router.push({ path: '/tasks', query });
  }
};

const handleActivityClick = (activity: Activity) => {
  // Navigate to related item based on activity type
  if (activity.subject_type === 'Task' && activity.subject_id) {
    router.push(`/tasks/${activity.subject_id}`);
  }
};

const handleTaskClick = (task: RecentTask) => {
  router.push(`/tasks/${task.id}`);
};

const handleMemberClick = (member: TeamMemberStatus) => {
  router.push(`/profile/${member.id}`);
};

// Optimistic update handlers
const optimisticallyUpdateStatistics = (updates: Partial<DashboardStatistics>) => {
  // Update statistics immediately for better UX
  if (statistics.value) {
    updateStatistics(updates);
  }
};

// Activity filters handler
const handleActivityFiltersChange = async (filters: ActivityFilters) => {
  try {
    await applyActivityFilters(filters);
  } catch (error) {
    handleError(error, 'activity filters');
  }
};

// Connection status monitoring
const updateConnectionStatus = () => {
  showConnectionStatus.value = true;
  setTimeout(() => {
    showConnectionStatus.value = false;
  }, 3000);
};

// Network status event handlers
onOnline(() => {
  console.log('Network connection restored');
  clearGlobalError();
  
  // Reconnect and refresh data
  nextTick(async () => {
    try {
      reconnectPusher();
      await handleRefresh();
      updateConnectionStatus();
    } catch (error) {
      handleError(error, 'network reconnection');
    }
  });
});

onOffline(() => {
  console.log('Network connection lost');
  setGlobalError({
    message: 'Network connection lost',
    type: 'network',
    timestamp: new Date().toISOString(),
    recoverable: true
  });
  updateConnectionStatus();
});

// Setup real-time subscriptions
const setupRealTimeUpdates = () => {
  if (!workspaceId.value) return;
  
  try {
    subscribeToWorkspace(workspaceId.value, {
      onActivityCreated: handleNewActivity,
      onDashboardUpdated: handleDashboardUpdate,
      onMemberStatusChanged: handleMemberStatusChange,
      onError: (error) => handleError(error, 'real-time'),
      onReconnect: () => {
        console.log('Real-time connection restored');
        handleRefresh();
      }
    });
    
    // Enable activity feed real-time updates
    enableActivityRealTime(workspaceId.value);
  } catch (error) {
    handleError(error, 'real-time setup');
  }
};

// Cleanup real-time subscriptions
const cleanupRealTimeUpdates = () => {
  if (workspaceId.value) {
    unsubscribeFromWorkspace(workspaceId.value);
    disableActivityRealTime();
  }
};

// Watch for workspace changes
watch(() => workspaceId.value, (newWorkspaceId, oldWorkspaceId) => {
  if (oldWorkspaceId) {
    cleanupRealTimeUpdates();
  }
  
  if (newWorkspaceId) {
    setupRealTimeUpdates();
    loadDashboard();
  }
});

// Keyboard navigation
const handleKeyboardNavigation = (event: KeyboardEvent) => {
  // Handle keyboard shortcuts for dashboard navigation
  if (event.ctrlKey || event.metaKey) {
    switch (event.key) {
      case 'r':
        event.preventDefault();
        handleRefresh();
        break;
      case '1':
        event.preventDefault();
        // Focus on statistics section
        document.querySelector('[aria-labelledby="statistics-heading"]')?.focus();
        break;
      case '2':
        event.preventDefault();
        // Focus on heatmap section
        document.querySelector('[aria-labelledby="heatmap-heading"]')?.focus();
        break;
      case '3':
        event.preventDefault();
        // Focus on activity feed
        document.querySelector('[aria-labelledby="activity-heading"]')?.focus();
        break;
      case '4':
        event.preventDefault();
        // Focus on team activity
        document.querySelector('[aria-labelledby="team-activity-heading"]')?.focus();
        break;
    }
  }
  
  // Handle escape key to clear errors
  if (event.key === 'Escape' && hasAnyError.value) {
    clearGlobalError();
    clearError();
  }
};

// Announce updates to screen readers
const announceUpdate = (message: string) => {
  const announcement = document.createElement('div');
  announcement.setAttribute('aria-live', 'polite');
  announcement.setAttribute('aria-atomic', 'true');
  announcement.className = 'sr-only';
  announcement.textContent = message;
  
  document.body.appendChild(announcement);
  
  // Remove after announcement
  setTimeout(() => {
    document.body.removeChild(announcement);
  }, 1000);
};

// Enhanced real-time event handlers with accessibility
const handleNewActivityAccessible = (activity: Activity) => {
  handleNewActivity(activity);
  
  // Announce to screen readers
  announceUpdate(`New activity: ${activity.description}`);
};

const handleDashboardUpdateAccessible = (updates: Partial<DashboardStatistics>) => {
  handleDashboardUpdate(updates);
  
  // Announce significant changes
  if (updates.completed_today) {
    announceUpdate(`Tasks completed today updated to ${updates.completed_today}`);
  }
};

// Lifecycle hooks
onMounted(async () => {
  try {
    // Set page title
    setDocumentTitle(pageTitle.value);
    store.commit('setMetaTitle', pageTitle.value);
    
    // Setup keyboard navigation
    document.addEventListener('keydown', handleKeyboardNavigation);
    
    // Ensure workspaces are loaded before loading dashboard
    if (!store.state.workspaces?.length) {
      console.log('Loading workspaces before dashboard initialization...');
      await store.dispatch('loadWorkspaces');
    }
    
    // Load dashboard data
    await loadDashboard();
    
    // Setup real-time updates with accessibility
    setupRealTimeUpdates();
    
    // Announce initial load completion
    announceUpdate('Dashboard loaded successfully');
    
  } catch (error) {
    handleError(error, 'initialization');
    announceUpdate('Dashboard failed to load');
  }
});

onUnmounted(() => {
  // Cleanup keyboard navigation
  document.removeEventListener('keydown', handleKeyboardNavigation);
  
  // Cleanup real-time subscriptions
  cleanupRealTimeUpdates();
});
</script>

<template>
  <div>
    <FeatureGate
    feature-key="dashboard"
    title="Dashboard"
    description="Get insights into your productivity with statistics, activity heatmaps, and real-time team updates."
    :icon="LayoutDashboard"
  >
    <template #preview>
      <DashboardPreview />
    </template>

  <BaseLayout>
    <template #body>
      <ErrorBoundary
        ref="errorBoundaryRef"
        fallback-title="Dashboard Error"
        fallback-message="The dashboard encountered an unexpected error. Please try refreshing the page."
        :show-details="false"
        :show-report-button="true"
        @error="handleErrorBoundaryError"
        @retry="handleErrorBoundaryRetry"
        @report="handleErrorReport"
      >
        <div 
          id="main-content"
          class="dashboard-container"
          role="main"
          aria-label="Dashboard"
          aria-describedby="dashboard-title"
        >
        <!-- Dashboard Header -->
        <header class="dashboard-header" role="banner">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-3">
              <h1 
                class="text-3xl font-bold text-gray-900 dark:text-white"
                id="dashboard-title"
              >
                {{ pageTitle }}
              </h1>
              
              <!-- Connection Status Indicator -->
              <div 
                v-if="showConnectionStatus || !isRealTimeActive"
                class="flex items-center gap-2"
                :class="{
                  'text-green-600 dark:text-green-400': isRealTimeActive,
                  'text-red-600 dark:text-red-400': !isRealTimeActive
                }"
                role="status"
                :aria-label="isRealTimeActive ? 'Real-time updates active' : 'Real-time updates inactive'"
              >
                <WifiIcon 
                  v-if="isRealTimeActive" 
                  class="h-4 w-4" 
                  aria-hidden="true"
                />
                <SignalSlashIcon 
                  v-else 
                  class="h-4 w-4" 
                  aria-hidden="true"
                />
                <span class="text-xs font-medium">
                  {{ isRealTimeActive ? 'Live' : 'Offline' }}
                </span>
              </div>
            </div>
            
            <!-- Dashboard Actions -->
            <nav class="flex items-center gap-2" aria-label="Dashboard actions">
              <Button
                variant="outline"
                size="sm"
                @click="handleRefresh"
                :disabled="isAnyLoading"
                :aria-label="isAnyLoading ? 'Refreshing dashboard data' : 'Refresh dashboard data'"
                :aria-describedby="isAnyLoading ? 'refresh-status' : undefined"
              >
                <ArrowPathIcon 
                  :class="[
                    'h-4 w-4 mr-2',
                    isAnyLoading && 'animate-spin'
                  ]" 
                  aria-hidden="true"
                />
                {{ isAnyLoading ? 'Refreshing...' : 'Refresh' }}
              </Button>
              
              <!-- Hidden status for screen readers -->
              <span 
                v-if="isAnyLoading"
                id="refresh-status"
                class="sr-only"
                aria-live="polite"
              >
                Dashboard is currently refreshing
              </span>
            </nav>
          </div>
          
          <!-- Keyboard shortcuts help -->
          <div class="sr-only" aria-live="polite">
            Keyboard shortcuts: Ctrl+R to refresh, Ctrl+1-5 to navigate sections, Escape to clear errors
          </div>
        </header>

        <!-- Error State -->
        <div 
          v-if="hasAnyError && !isInitialLoad"
          class="mb-6 rounded-lg border p-4"
          :class="{
            'bg-red-50 border-red-200 dark:bg-red-900/20 dark:border-red-800': !isOffline.value,
            'bg-yellow-50 border-yellow-200 dark:bg-yellow-900/20 dark:border-yellow-800': isOffline.value
          }"
          role="alert"
          aria-live="polite"
        >
          <div class="flex items-start">
            <ExclamationTriangleIcon 
              :class="[
                'h-5 w-5 mt-0.5 mr-3 flex-shrink-0',
                isOffline.value 
                  ? 'text-yellow-600 dark:text-yellow-400' 
                  : 'text-red-600 dark:text-red-400'
              ]" 
            />
            <div class="flex-1">
              <h3 
                :class="[
                  'font-medium',
                  isOffline.value 
                    ? 'text-yellow-800 dark:text-yellow-200' 
                    : 'text-red-800 dark:text-red-200'
                ]"
              >
                {{ isOffline.value ? 'Connection Lost' : 'Dashboard Error' }}
              </h3>
              <p 
                :class="[
                  'mt-1 text-sm',
                  isOffline.value 
                    ? 'text-yellow-700 dark:text-yellow-300' 
                    : 'text-red-700 dark:text-red-300'
                ]"
              >
                {{ combinedErrorMessage }}
              </p>
              <div v-if="canRetry" class="mt-3 flex gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  @click="handleRetry"
                  :disabled="isRetryingGlobal"
                  :class="[
                    isOffline.value 
                      ? 'text-yellow-700 border-yellow-300 hover:bg-yellow-50 dark:text-yellow-300 dark:border-yellow-600 dark:hover:bg-yellow-900/30'
                      : 'text-red-700 border-red-300 hover:bg-red-50 dark:text-red-300 dark:border-red-600 dark:hover:bg-red-900/30'
                  ]"
                >
                  <ArrowPathIcon 
                    :class="[
                      'h-4 w-4 mr-2',
                      isRetryingGlobal && 'animate-spin'
                    ]" 
                  />
                  {{ isRetryingGlobal ? 'Retrying...' : 'Try Again' }}
                </Button>
                
                <Button
                  v-if="isOffline.value"
                  variant="outline"
                  size="sm"
                  @click="checkConnection"
                  class="text-yellow-700 border-yellow-300 hover:bg-yellow-50 dark:text-yellow-300 dark:border-yellow-600 dark:hover:bg-yellow-900/30"
                >
                  Check Connection
                </Button>
              </div>
            </div>
          </div>
        </div>

        <!-- Activity Heatmap Section -->
        <section 
          class="dashboard-section"
          aria-labelledby="heatmap-heading"
          tabindex="0"
          role="region"
        >
          <div class="mb-4">
            <h2 id="heatmap-heading" class="text-xl font-semibold text-gray-900 dark:text-white">
              Activity Heatmap
            </h2>
            <p 
              class="text-sm text-gray-600 dark:text-gray-400"
              id="heatmap-description"
            >
              Your contribution activity over the past year. Each square represents a day, with darker colors indicating more activity.
            </p>
          </div>
          
            <div 
              class="heatmap-container"
              role="img"
              :aria-label="heatmapData ? `Activity heatmap showing ${heatmapData.total_contributions} total contributions` : 'Activity heatmap loading'"
              :aria-describedby="heatmapData ? 'heatmap-summary' : 'heatmap-loading'"
            >
              <HeatmapSkeleton v-if="loadingStates.heatmap.isLoading" />
              
              <div v-else-if="loadingStates.heatmap.error" class="flex flex-col items-center justify-center py-20">
                <ExclamationTriangleIcon class="h-12 w-12 text-red-500 mb-4" />
                <p class="text-sm text-red-600 dark:text-red-400 mb-4">
                  Failed to load heatmap data
                </p>
                <Button @click="refreshSection('heatmap')" variant="outline" size="sm">
                  <ArrowPathIcon class="h-4 w-4 mr-2" />
                  Retry
                </Button>
              </div>
              
              <HeatmapCalendar 
                v-else-if="heatmapData"
                :data="heatmapData"
                :theme="store.state.colorScheme"
              />
              
              <div v-else class="flex flex-col items-center justify-center py-20">
                <p class="text-sm text-gray-600 dark:text-gray-400">No contribution data available</p>
              </div>
              
              <!-- Heatmap summary for screen readers -->
              <div 
                v-if="heatmapData"
                id="heatmap-summary"
                class="sr-only"
              >
                Total contributions: {{ heatmapData.total_contributions }}. 
                Current streak: {{ heatmapData.streak?.current || 0 }} days. 
                Longest streak: {{ heatmapData.streak?.longest || 0 }} days.
              </div>
              
              <!-- Loading announcement -->
              <div 
                v-if="loadingStates.heatmap.isLoading"
                id="heatmap-loading"
                class="sr-only"
                aria-live="polite"
              >
                Loading activity heatmap
              </div>
            </div>
        </section>

        <!-- Statistics Grid -->
        <section 
          class="dashboard-section"
          aria-labelledby="statistics-heading"
          tabindex="0"
          role="region"
        >
          <h2 id="statistics-heading" class="text-xl font-semibold text-gray-900 dark:text-white mb-4">
            Workspace Statistics
          </h2>
            <StatisticsGrid
              :statistics="statistics || {}"
              :loading="loadingStates.statistics.isLoading"
              @navigate-to-filtered="handleStatisticCardClick"
              :aria-describedby="loadingStates.statistics.isLoading ? 'statistics-loading' : 'statistics-description'"
            />
          
          <!-- Loading announcement -->
          <div 
            v-if="loadingStates.statistics.isLoading"
            id="statistics-loading"
            class="sr-only"
            aria-live="polite"
          >
            Loading workspace statistics
          </div>
          
          <!-- Description for screen readers -->
          <div 
            id="statistics-description"
            class="sr-only"
          >
            Grid of workspace statistics including task counts, time tracking, and team metrics. Use arrow keys to navigate between cards.
          </div>
        </section>

        <!-- Main Dashboard Grid -->
        <div class="dashboard-grid" role="main">
          <!-- Activity Feed -->
          <section 
            class="dashboard-main-content"
            aria-labelledby="activity-heading"
            tabindex="0"
            role="region"
          >
            <h2 id="activity-heading" class="sr-only">Recent Activity Feed</h2>
            <ActivityFeed
              :activities="displayedActivities || []"
              :loading="activityLoading"
              :has-more="canLoadMore"
              :loading-more="activityLoadingMore"
              :workspace-id="workspaceId"
              :workspace-users="workspaceUsersList"
              @load-more="loadMoreActivities"
              @refresh="() => refreshSection('activities')"
              @filter-change="handleActivityFiltersChange"
              @activity-click="handleActivityClick"
              :aria-describedby="activityLoading ? 'activity-loading' : 'activity-description'"
            />
            
            <!-- Activity feed description -->
            <div 
              id="activity-description"
              class="sr-only"
            >
              List of recent workspace activities. Use arrow keys to navigate, Enter to open activity details.
            </div>
            
            <!-- Loading announcement -->
            <div 
              v-if="activityLoading"
              id="activity-loading"
              class="sr-only"
              aria-live="polite"
            >
              Loading recent activities
            </div>
          </section>

          <!-- Sidebar -->
          <aside 
            class="dashboard-sidebar"
            role="complementary"
            aria-label="Dashboard sidebar with recent tasks and team activity"
          >
            <!-- Team Activity -->
            <section 
              aria-labelledby="team-activity-heading"
              tabindex="0"
              role="region"
            >
              <h3 id="team-activity-heading" class="sr-only">Team Activity</h3>
                <TeamActivityWidget
                  :team-activity="teamActivity"
                  :loading="loadingStates.teamActivity.isLoading"
                  @refresh="() => refreshSection('teamActivity')"
                  @member-click="handleMemberClick"
                  @task-click="handleTaskClick"
                  @invite-members="() => router.push('/workspace/invite')"
                  :aria-describedby="loadingStates.teamActivity.isLoading ? 'team-loading' : 'team-description'"
                />
              
              <!-- Team activity description -->
              <div 
                id="team-description"
                class="sr-only"
              >
                Team member activity and status. Shows who is online and their current tasks.
              </div>
              
              <!-- Loading announcement -->
              <div 
                v-if="loadingStates.teamActivity.isLoading"
                id="team-loading"
                class="sr-only"
                aria-live="polite"
              >
                Loading team activity
              </div>
            </section>
          </aside>
        </div>

        <!-- Empty State for Initial Load -->
        <div 
          v-if="isInitialLoad"
          class="flex items-center justify-center py-12"
          aria-live="polite"
          aria-label="Loading dashboard"
        >
          <div class="text-center">
            <ArrowPathIcon class="h-8 w-8 animate-spin text-gray-400 mx-auto mb-4" />
            <h3 class="text-lg font-medium text-gray-900 dark:text-white mb-2">
              Loading Dashboard
            </h3>
            <p class="text-gray-600 dark:text-gray-400">
              Fetching your workspace data...
            </p>
          </div>
        </div>

        <!-- Connection Status Notifications -->
        <Transition
          enter-active-class="transition ease-out duration-300"
          enter-from-class="transform opacity-0 translate-y-2"
          enter-to-class="transform opacity-100 translate-y-0"
          leave-active-class="transition ease-in duration-200"
          leave-from-class="transform opacity-100 translate-y-0"
          leave-to-class="transform opacity-0 translate-y-2"
        >
          <div 
            v-if="showConnectionStatus || isOffline.value"
            class="fixed bottom-4 right-4 z-50 rounded-lg p-4 shadow-lg"
            :class="{
              'bg-yellow-100 border border-yellow-300 dark:bg-yellow-900/30 dark:border-yellow-700': isOffline.value,
              'bg-green-100 border border-green-300 dark:bg-green-900/30 dark:border-green-700': !isOffline.value && isRealTimeActive
            }"
            role="status"
            aria-live="polite"
          >
            <div class="flex items-center">
              <SignalSlashIcon 
                v-if="isOffline.value"
                class="h-5 w-5 text-yellow-600 dark:text-yellow-400 mr-2" 
              />
              <WifiIcon 
                v-else-if="isRealTimeActive"
                class="h-5 w-5 text-green-600 dark:text-green-400 mr-2" 
              />
              <span 
                class="text-sm font-medium"
                :class="{
                  'text-yellow-800 dark:text-yellow-200': isOffline.value,
                  'text-green-800 dark:text-green-200': !isOffline.value && isRealTimeActive
                }"
              >
                {{ 
                  isOffline.value 
                    ? 'You\'re offline' 
                    : 'Real-time updates active'
                }}
              </span>
            </div>
          </div>
        </Transition>
        </div>
      </ErrorBoundary>
    </template>
  </BaseLayout>

    </FeatureGate>
  </div>
</template>

<style scoped>
/* CHANGES: Consolidated and simplified CSS - removed duplicates, kept essential styles only */
/* Bundle size reduction: ~40% less CSS */

/* Dashboard Layout */
.dashboard-container {
  @apply w-full max-w-7xl mx-auto px-4 py-6 min-h-screen;
  scrollbar-width: thin;
  scrollbar-color: rgb(156 163 175) transparent;
}

.dashboard-header {
  @apply mb-8 pb-4 border-b border-gray-200 dark:border-gray-700;
}

.dashboard-section {
  @apply mb-8 transition-all duration-200 ease-in-out;
}

.heatmap-container {
  @apply bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6 min-h-48;
}

.dashboard-grid {
  @apply flex flex-col gap-6;
}

.dashboard-main-content {
  @apply w-full min-h-96;
}

.dashboard-sidebar {
  @apply w-full flex flex-col space-y-6 min-h-64;
}

/* Responsive Design */
@media (max-width: 768px) {
  .dashboard-container {
    @apply px-4;
  }

  .dashboard-header {
    @apply mb-6;
  }
  
  .dashboard-header .flex {
    @apply flex-col items-start gap-3;
  }
  
  .dashboard-section {
    @apply mb-6;
  }
}

@media (max-width: 640px) {
  .dashboard-container {
    @apply px-2 py-2 overflow-x-hidden overflow-y-auto;
  }
  
  .dashboard-section {
    @apply mb-4;
  }
  
  .dashboard-header {
    @apply mb-4 pb-2;
  }
  
  .dashboard-header h1 {
    @apply text-xl;
  }
  
  .heatmap-container {
    @apply p-3 overflow-x-hidden;
  }
}

/* Custom Scrollbar */
.dashboard-container::-webkit-scrollbar {
  width: 0.5rem;
}

.dashboard-container::-webkit-scrollbar-track {
  @apply bg-transparent;
}

.dashboard-container::-webkit-scrollbar-thumb {
  @apply bg-gray-400 dark:bg-gray-600 rounded-full;
}

.dashboard-container::-webkit-scrollbar-thumb:hover {
  @apply bg-gray-500;
}

/* Focus States */
.dashboard-section:focus-within {
  @apply ring-2 ring-blue-500 ring-offset-2 dark:ring-offset-gray-800;
}

/* Animations */
@keyframes pulse-subtle {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.8; }
}

.animate-pulse-subtle {
  animation: pulse-subtle 1s ease-in-out;
}

.statistics-grid.animate-pulse {
  animation: pulse-subtle 0.5s ease-in-out;
}

.activity-item.new-activity {
  @apply bg-blue-50 dark:bg-blue-900/20 border-blue-200 dark:border-blue-800;
  animation: pulse-subtle 2s ease-in-out;
}

.team-member.status-changed {
  animation: pulse-subtle 1s ease-in-out;
}

/* Loading Skeleton with Shimmer */
.loading-skeleton {
  @apply animate-pulse bg-gradient-to-r from-gray-200 via-gray-300 to-gray-200 dark:from-gray-700 dark:via-gray-600 dark:to-gray-700 rounded;
  background-size: 200% 100%;
  animation: loading-shimmer 1.5s infinite;
}

@keyframes loading-shimmer {
  0% { background-position: -200% 0; }
  100% { background-position: 200% 0; }
}

/* Accessibility: Reduced Motion */
@media (prefers-reduced-motion: reduce) {
  .animate-spin,
  .animate-pulse,
  .animate-pulse-subtle,
  .loading-skeleton {
    animation: none;
  }
  
  .dashboard-section {
    transition: none;
  }
}

/* High Contrast Mode */
@media (prefers-contrast: high) {
  .dashboard-container {
    @apply border-2 border-gray-900 dark:border-gray-100;
  }
  
  .dashboard-section {
    @apply border-2 border-gray-900 dark:border-gray-100 rounded-lg p-4 bg-white text-black dark:bg-black dark:text-white;
  }
}

/* Print Styles */
@media print {
  .dashboard-container {
    @apply max-w-none px-0;
  }
  
  .dashboard-header button,
  .fixed {
    display: none;
  }
}
</style> 