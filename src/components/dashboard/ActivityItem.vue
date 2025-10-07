<template>
  <div 
    :class="itemClasses"
    @click="handleClick"
    @keydown.enter="handleClick"
    @keydown.space.prevent="handleClick"
    tabindex="0"
    role="button"
    :aria-label="accessibilityLabel"
  >
    <div class="flex items-start space-x-3">
      <!-- Activity Icon -->
      <div :class="iconClasses">
        <component :is="activityIcon" class="h-4 w-4" />
      </div>
      
      <!-- Activity Content -->
      <div class="flex-1 min-w-0">
        <div class="flex items-center justify-between">
          <div class="flex items-center space-x-2">
            <!-- User Avatar -->
            <img
              v-if="activity.user.avatar"
              :src="activity.user.avatar"
              :alt="activity.user.name"
              class="w-6 h-6 rounded-full"
            />
            <div
              v-else
              class="w-6 h-6 rounded-full bg-gray-300 dark:bg-gray-600 flex items-center justify-center"
            >
              <span class="text-xs font-medium text-gray-600 dark:text-gray-300">
                {{ userInitials }}
              </span>
            </div>
            
            <!-- User Name -->
            <span class="text-sm font-medium text-gray-900 dark:text-white">
              {{ activity.user.name }}
            </span>
          </div>
          
          <!-- Timestamp -->
          <time 
            :datetime="activity.created_at"
            class="text-xs text-gray-500 dark:text-gray-400"
            :title="formattedDate"
          >
            {{ activity.timestamp_human }}
          </time>
        </div>
        
        <!-- Activity Description -->
        <p class="mt-1 text-sm text-gray-600 dark:text-gray-300">
          {{ activity.description }}
        </p>
        
        <!-- Activity Metadata -->
        <div v-if="hasMetadata" class="mt-2 flex flex-wrap gap-2">
          <span
            v-for="(value, key) in displayMetadata"
            :key="key"
            class="inline-flex items-center px-2 py-1 rounded-md text-xs bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300"
          >
            <span class="font-medium">{{ formatMetadataKey(key) }}:</span>
            <span class="ml-1">{{ value }}</span>
          </span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { Activity, ActivityType } from '@/types/dashboard';
import { cn } from '@/utils';
import {
  PlusIcon,
  PencilIcon,
  CheckIcon,
  TrashIcon,
  ArrowUturnLeftIcon,
  UserPlusIcon,
  UserMinusIcon,
  PlayIcon,
  StopIcon,
  ChatBubbleLeftIcon,
  FolderIcon,
  DocumentIcon,
  ArrowPathIcon
} from '@heroicons/vue/24/outline';

interface Props {
  activity: Activity;
}

const props = defineProps<Props>();

const emit = defineEmits<{
  click: [activity: Activity];
}>();

const userInitials = computed(() => {
  return props.activity.user.name
    .split(' ')
    .map(name => name.charAt(0))
    .join('')
    .toUpperCase()
    .slice(0, 2);
});

const formattedDate = computed(() => {
  return new Date(props.activity.created_at).toLocaleString();
});

const activityIcon = computed(() => {
  const iconMap: Record<string, any> = {
    task_created: PlusIcon,
    task_updated: PencilIcon,
    task_completed: CheckIcon,
    task_deleted: TrashIcon,
    task_restored: ArrowUturnLeftIcon,
    task_status_changed: ArrowPathIcon,
    task_assigned: UserPlusIcon,
    task_timer_started: PlayIcon,
    task_timer_stopped: StopIcon,
    comment_created: ChatBubbleLeftIcon,
    comment_updated: PencilIcon,
    comment_deleted: TrashIcon,
    category_created: FolderIcon,
    category_updated: PencilIcon,
    category_deleted: TrashIcon,
    file_uploaded: DocumentIcon,
    file_deleted: TrashIcon,
    member_joined: UserPlusIcon,
    member_left: UserMinusIcon,
    routine_completed: CheckIcon
  };
  
  return iconMap[props.activity.type] || PlusIcon;
});

const iconClasses = computed(() => {
  const colorMap: Record<string, string> = {
    task_created: 'bg-green-100 text-green-600 dark:bg-green-900/20 dark:text-green-400',
    task_updated: 'bg-blue-100 text-blue-600 dark:bg-blue-900/20 dark:text-blue-400',
    task_completed: 'bg-green-100 text-green-600 dark:bg-green-900/20 dark:text-green-400',
    task_deleted: 'bg-red-100 text-red-600 dark:bg-red-900/20 dark:text-red-400',
    task_restored: 'bg-blue-100 text-blue-600 dark:bg-blue-900/20 dark:text-blue-400',
    task_status_changed: 'bg-orange-100 text-orange-600 dark:bg-orange-900/20 dark:text-orange-400',
    task_assigned: 'bg-purple-100 text-purple-600 dark:bg-purple-900/20 dark:text-purple-400',
    task_timer_started: 'bg-green-100 text-green-600 dark:bg-green-900/20 dark:text-green-400',
    task_timer_stopped: 'bg-red-100 text-red-600 dark:bg-red-900/20 dark:text-red-400',
    comment_created: 'bg-blue-100 text-blue-600 dark:bg-blue-900/20 dark:text-blue-400',
    comment_updated: 'bg-orange-100 text-orange-600 dark:bg-orange-900/20 dark:text-orange-400',
    comment_deleted: 'bg-red-100 text-red-600 dark:bg-red-900/20 dark:text-red-400',
    category_created: 'bg-indigo-100 text-indigo-600 dark:bg-indigo-900/20 dark:text-indigo-400',
    category_updated: 'bg-orange-100 text-orange-600 dark:bg-orange-900/20 dark:text-orange-400',
    category_deleted: 'bg-red-100 text-red-600 dark:bg-red-900/20 dark:text-red-400',
    file_uploaded: 'bg-green-100 text-green-600 dark:bg-green-900/20 dark:text-green-400',
    file_deleted: 'bg-red-100 text-red-600 dark:bg-red-900/20 dark:text-red-400',
    member_joined: 'bg-green-100 text-green-600 dark:bg-green-900/20 dark:text-green-400',
    member_left: 'bg-red-100 text-red-600 dark:bg-red-900/20 dark:text-red-400',
    routine_completed: 'bg-green-100 text-green-600 dark:bg-green-900/20 dark:text-green-400'
  };
  
  const baseClasses = 'flex items-center justify-center w-8 h-8 rounded-full';
  const colorClass = colorMap[props.activity.type] || 'bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-400';
  
  return cn(baseClasses, colorClass);
});

const itemClasses = computed(() => {
  return cn(
    'p-3 rounded-lg border border-gray-200 dark:border-gray-700',
    'hover:bg-gray-50 dark:hover:bg-gray-700/50 transition-colors duration-150',
    'cursor-pointer focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400',
    'focus:ring-offset-2 dark:focus:ring-offset-gray-800'
  );
});

const hasMetadata = computed(() => {
  return props.activity.metadata && Object.keys(props.activity.metadata).length > 0;
});

const displayMetadata = computed(() => {
  if (!hasMetadata.value) return {};
  
  // Filter and format metadata for display
  const metadata = props.activity.metadata;
  const displayData: Record<string, any> = {};
  
  // Common metadata fields to display
  const displayFields = [
    'task_title',
    'old_status',
    'new_status',
    'category_name',
    'file_name',
    'comment_content'
  ];
  
  displayFields.forEach(field => {
    if (metadata[field] !== undefined) {
      displayData[field] = metadata[field];
    }
  });
  
  return displayData;
});

const formatMetadataKey = (key: string): string => {
  return key
    .replace(/_/g, ' ')
    .replace(/\b\w/g, l => l.toUpperCase());
};

const accessibilityLabel = computed(() => {
  return `${props.activity.user.name} ${props.activity.description} ${props.activity.timestamp_human}`;
});

const handleClick = () => {
  emit('click', props.activity);
};
</script>

<style scoped>
/* Additional custom styles if needed */
</style>
</template>
</invoke>