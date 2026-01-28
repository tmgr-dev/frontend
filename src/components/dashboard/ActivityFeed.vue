<template>
  <div class="activity-feed">
    <div class="activity-header flex items-center justify-between mb-4">
      <h2 class="text-lg font-semibold text-gray-900 dark:text-white">
        Recent Activity
      </h2>
      <div class="flex items-center space-x-2">
        <ActivityFilters 
          v-model="filters" 
          @update:model-value="handleFiltersChange"
          :workspace-users="workspaceUsers"
        />
        <Button
          variant="ghost"
          size="sm"
          @click="refreshActivities"
          :disabled="loading"
          class="p-2"
          aria-label="Refresh activities"
        >
          <ArrowPathIcon 
            :class="[
              'h-4 w-4',
              loading && 'animate-spin'
            ]" 
          />
        </Button>
      </div>
    </div>
    
    <div class="activity-list space-y-3">
      <!-- Loading skeleton -->
      <template v-if="loading && activities.length === 0">
        <ActivityItemSkeleton 
          v-for="i in 5" 
          :key="`skeleton-${i}`" 
        />
      </template>
      
      <!-- Activities -->
      <template v-else-if="activities.length > 0">
        <ActivityItem
          v-for="activity in activities"
          :key="activity.id"
          :activity="activity"
          @click="handleActivityClick(activity)"
        />
        
        <!-- Load more button -->
        <div v-if="hasMore" class="flex justify-center pt-4">
          <Button
            variant="outline"
            :loading="loadingMore"
            @click="handleLoadMore"
            :disabled="loadingMore"
          >
            <template v-if="loadingMore">
              <ArrowPathIcon class="h-4 w-4 animate-spin mr-2" />
              Loading...
            </template>
            <template v-else>
              Load More Activities
            </template>
          </Button>
        </div>
      </template>
      
      <!-- Empty state -->
      <template v-else>
        <EmptyState
          icon="activity"
          title="No activities yet"
          description="Activities will appear here as team members work on tasks."
        />
      </template>
    </div>
    
    <!-- Real-time indicator -->
    <div 
      v-if="isRealTimeConnected" 
      class="flex items-center justify-center mt-4 text-xs text-green-600 dark:text-green-400"
    >
      <div class="w-2 h-2 bg-green-500 rounded-full animate-pulse mr-2"></div>
      Live updates enabled
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import type { Activity, ActivityFilters as ActivityFiltersType } from '@/types/dashboard';
import { usePusher } from '@/composable/usePusher';
import Button from '@/components/ui/button/Button.vue';
import EmptyState from '@/components/EmptyState.vue';
import ActivityItem from './ActivityItem.vue';
import ActivityItemSkeleton from './ActivityItemSkeleton.vue';
import ActivityFilters from './ActivityFilters.vue';
import { ArrowPathIcon } from '@heroicons/vue/24/outline';

interface Props {
  activities: Activity[];
  loading: boolean;
  hasMore?: boolean;
  loadingMore?: boolean;
  workspaceId: number;
  workspaceUsers?: Array<{ id: number; name: string; }>;
}

const props = withDefaults(defineProps<Props>(), {
  hasMore: false,
  loadingMore: false,
  workspaceUsers: () => []
});

const emit = defineEmits<{
  'load-more': [];
  'refresh': [];
  'filter-change': [filters: ActivityFiltersType];
  'activity-click': [activity: Activity];
}>();

const filters = ref<ActivityFiltersType>({
  type: 'all',
  user_id: 'all',
  limit: 20
});

const { subscribe, unsubscribe, isConnected } = usePusher();

const isRealTimeConnected = computed(() => isConnected.value);

const handleFiltersChange = (newFilters: ActivityFiltersType) => {
  emit('filter-change', newFilters);
};

const handleLoadMore = () => {
  emit('load-more');
};

const refreshActivities = () => {
  emit('refresh');
};

const handleActivityClick = (activity: Activity) => {
  emit('activity-click', activity);
};

const handleNewActivity = (activity: Activity) => {
  // The parent component will handle adding the new activity
  // We could show a toast notification here
  console.log('New activity received:', activity);
};

const handleDashboardUpdate = (data: any) => {
  // Handle dashboard updates if needed
  console.log('Dashboard updated:', data);
};

// Watch for workspace changes to resubscribe
watch(() => props.workspaceId, (newWorkspaceId, oldWorkspaceId) => {
  if (oldWorkspaceId) {
    unsubscribe(`workspace.${oldWorkspaceId}`);
  }
  if (newWorkspaceId) {
    subscribe(`workspace.${newWorkspaceId}`, {
      onActivityCreated: handleNewActivity,
      onDashboardUpdated: handleDashboardUpdate
    });
  }
}, { immediate: true });

onMounted(() => {
  if (props.workspaceId) {
    subscribe(`workspace.${props.workspaceId}`, {
      onActivityCreated: handleNewActivity,
      onDashboardUpdated: handleDashboardUpdate
    });
  }
});

onUnmounted(() => {
  if (props.workspaceId) {
    unsubscribe(`workspace.${props.workspaceId}`);
  }
});
</script>

<style scoped>
.activity-feed {
  @apply bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6;
}

.activity-header {
  @apply border-b border-gray-200 dark:border-gray-700 pb-4 mb-4;
}

.activity-list {
  @apply max-h-[600px] min-h-64 overflow-y-auto;
}

.activity-list > * + * {
  @apply border-t border-gray-100 dark:border-gray-700/50;
}

/* Custom scrollbar */
.activity-list::-webkit-scrollbar {
  @apply w-2;
}

.activity-list::-webkit-scrollbar-track {
  @apply bg-gray-100 dark:bg-gray-700 rounded;
}

.activity-list::-webkit-scrollbar-thumb {
  @apply bg-gray-300 dark:bg-gray-600 rounded hover:bg-gray-400 dark:hover:bg-gray-500;
}
</style>