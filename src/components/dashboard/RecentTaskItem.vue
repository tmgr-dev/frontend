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
    <div class="flex items-start justify-between">
      <div class="flex-1 min-w-0">
        <!-- Task Title and Status -->
        <div class="flex items-center space-x-2 mb-2">
          <div 
            :class="statusIndicatorClasses"
            :title="task.status?.name || 'Unknown'"
          ></div>
          <h4 class="text-sm font-medium text-gray-900 dark:text-white truncate">
            {{ task.title }}
          </h4>
          <div v-if="task.is_overdue" class="flex items-center">
            <ExclamationTriangleIcon class="h-4 w-4 text-red-500" />
            <span class="sr-only">Overdue</span>
          </div>
        </div>
        
        <!-- Task Metadata -->
        <div class="flex items-center space-x-4 text-xs text-gray-500 dark:text-gray-400">
          <!-- Category -->
          <div v-if="task.category" class="flex items-center space-x-1">
            <div 
              class="w-2 h-2 rounded-full"
              :style="{ backgroundColor: task.category.color || '#6B7280' }"
            ></div>
            <span>{{ task.category.name }}</span>
          </div>
          
          <!-- Assignee -->
          <div class="flex items-center space-x-1">
            <UserIcon class="h-3 w-3" />
            <span>{{ task.user.name }}</span>
          </div>
          
          <!-- Priority -->
          <div v-if="task.priority" class="flex items-center space-x-1">
            <component :is="priorityIcon" :class="priorityIconClasses" />
            <span class="capitalize">{{ task.priority }}</span>
          </div>
          
          <!-- Last Updated -->
          <div class="flex items-center space-x-1">
            <ClockIcon class="h-3 w-3" />
            <time 
              :datetime="task.updated_at"
              :title="formattedDate"
            >
              {{ task.updated_at_human }}
            </time>
          </div>
        </div>
        
        <!-- Assignees (if multiple) -->
        <div v-if="task.assignees && task.assignees.length > 1" class="flex items-center mt-2 space-x-1">
          <span class="text-xs text-gray-500 dark:text-gray-400">Assignees:</span>
          <div class="flex -space-x-1">
            <img
              v-for="assignee in task.assignees.slice(0, 3)"
              :key="assignee.id"
              :src="assignee.avatar || generateAvatarUrl(assignee.name)"
              :alt="assignee.name"
              :title="assignee.name"
              class="w-5 h-5 rounded-full border border-white dark:border-gray-800"
            />
            <div
              v-if="task.assignees.length > 3"
              class="w-5 h-5 rounded-full bg-gray-300 dark:bg-gray-600 border border-white dark:border-gray-800 flex items-center justify-center"
              :title="`+${task.assignees.length - 3} more`"
            >
              <span class="text-xs font-medium text-gray-600 dark:text-gray-300">
                +{{ task.assignees.length - 3 }}
              </span>
            </div>
          </div>
        </div>
      </div>
      
      <!-- Actions -->
      <div class="flex items-center space-x-2 ml-4">
        <!-- Timer Button -->
        <Button
          v-if="isFeatureEnabled('task.countdown')"
          variant="ghost"
          size="sm"
          @click.stop="handleTimerToggle"
          :class="timerButtonClasses"
          :title="task.timer_running ? 'Stop timer' : 'Start timer'"
        >
          <component :is="timerIcon" class="h-4 w-4" />
          <span v-if="task.timer_running && timerDuration" class="ml-1 text-xs">
            {{ timerDuration }}
          </span>
        </Button>
        
        <!-- Status Dropdown -->
        <Select 
          v-if="task.status"
          :model-value="task.status.id.toString()" 
          @update:model-value="handleStatusChange"
        >
          <SelectTrigger class="w-24 h-8 text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem 
              v-for="status in availableStatuses" 
              :key="status.id" 
              :value="status.id.toString()"
            >
              <div class="flex items-center space-x-2">
                <div 
                  class="w-2 h-2 rounded-full"
                  :style="{ backgroundColor: status.color || '#6B7280' }"
                ></div>
                <span>{{ status.name }}</span>
              </div>
            </SelectItem>
          </SelectContent>
        </Select>
        
        <!-- Fallback when no status -->
        <div v-else class="w-24 h-8 text-xs flex items-center justify-center text-gray-500 dark:text-gray-400 border border-gray-200 dark:border-gray-700 rounded-md">
          No Status
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted, watch } from 'vue';
import type { RecentTask } from '@/types/dashboard';
import { cn } from '@/utils';
import Button from '@/components/ui/button/Button.vue';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  PlayIcon,
  StopIcon,
  ClockIcon,
  UserIcon,
  ExclamationTriangleIcon,
  ArrowUpIcon,
  ArrowRightIcon,
  ArrowDownIcon
} from '@heroicons/vue/24/outline';
import { useFeatureToggles } from '@/composable/useFeatureToggles';

interface Props {
  task: RecentTask;
  availableStatuses?: Array<{ id: number; name: string; color?: string; type: string; }>;
}

const props = withDefaults(defineProps<Props>(), {
  availableStatuses: () => [
    { id: 1, name: 'To Do', color: '#6B7280', type: 'pending' },
    { id: 2, name: 'In Progress', color: '#3B82F6', type: 'active' },
    { id: 3, name: 'Done', color: '#10B981', type: 'archived' }
  ]
});

const emit = defineEmits<{
  click: [task: RecentTask];
  'timer-toggle': [task: RecentTask];
  'status-change': [statusId: number];
}>();

const { isFeatureEnabled } = useFeatureToggles();

const timerInterval = ref<NodeJS.Timeout | null>(null);
const timerDuration = ref<string>('');

const formattedDate = computed(() => {
  return new Date(props.task.updated_at).toLocaleString();
});

const priorityIcon = computed(() => {
  const iconMap = {
    urgent: ExclamationTriangleIcon,
    high: ArrowUpIcon,
    medium: ArrowRightIcon,
    low: ArrowDownIcon
  };
  return iconMap[props.task.priority || 'medium'];
});

const priorityIconClasses = computed(() => {
  const colorMap = {
    urgent: 'text-red-500',
    high: 'text-orange-500',
    medium: 'text-yellow-500',
    low: 'text-green-500'
  };
  return cn('h-3 w-3', colorMap[props.task.priority || 'medium']);
});

const timerIcon = computed(() => {
  return props.task.timer_running ? StopIcon : PlayIcon;
});

const timerButtonClasses = computed(() => {
  return cn(
    'p-1',
    props.task.timer_running 
      ? 'text-red-600 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300' 
      : 'text-green-600 hover:text-green-700 dark:text-green-400 dark:hover:text-green-300'
  );
});

const statusIndicatorClasses = computed(() => {
  return cn(
    'w-3 h-3 rounded-full flex-shrink-0',
    `bg-[${props.task.status?.color || '#6B7280'}]`
  );
});

const isTimeExceeded = computed(() => {
  const approximatelyTime = getApproximatelyTime();
  if (!approximatelyTime || approximatelyTime <= 0) {
    return false;
  }
  const currentTime = (props.task as any).common_time || 0;
  return currentTime > approximatelyTime;
});

const getApproximatelyTime = () => {
  const task = props.task as any;
  if (task.approximately_time) {
    return parseInt(String(task.approximately_time), 10);
  }
  const setting = task.settings?.find((s: any) => s.key === 'approximately_time');
  if (setting) {
    return parseInt(String(setting.value || setting.pivot?.value), 10);
  }
  return 0;
};

const itemClasses = computed(() => {
  return cn(
    'p-3 rounded-lg border border-gray-200 dark:border-gray-700',
    'hover:bg-gray-50 dark:hover:bg-gray-700/50 transition-colors duration-150',
    'cursor-pointer focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400',
    'focus:ring-offset-2 dark:focus:ring-offset-gray-800',
    props.task.is_overdue && 'border-red-200 dark:border-red-800 bg-red-50/50 dark:bg-red-900/10',
    isTimeExceeded.value && 'border-l-4 border-l-red-500 dark:border-l-red-400'
  );
});

const accessibilityLabel = computed(() => {
  const parts = [
    `Task: ${props.task.title}`,
    `Status: ${props.task.status?.name || 'Unknown'}`,
    `Assigned to: ${props.task.user.name}`,
    `Updated: ${props.task.updated_at_human}`
  ];
  
  if (props.task.is_overdue) {
    parts.push('Overdue');
  }
  
  if (props.task.timer_running) {
    parts.push('Timer running');
  }
  
  return parts.join(', ');
});

const generateAvatarUrl = (name: string): string => {
  // Generate a simple avatar URL based on name
  const initials = name.split(' ').map(n => n.charAt(0)).join('').toUpperCase();
  return `https://ui-avatars.com/api/?name=${encodeURIComponent(initials)}&size=20&background=6B7280&color=fff`;
};

const updateTimerDuration = () => {
  if (!props.task.timer_running || !props.task.timer_started_at) {
    timerDuration.value = '';
    return;
  }
  
  const startTime = new Date(props.task.timer_started_at).getTime();
  const now = Date.now();
  const duration = Math.floor((now - startTime) / 1000);
  
  const hours = Math.floor(duration / 3600);
  const minutes = Math.floor((duration % 3600) / 60);
  const seconds = duration % 60;
  
  if (hours > 0) {
    timerDuration.value = `${hours}:${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
  } else {
    timerDuration.value = `${minutes}:${seconds.toString().padStart(2, '0')}`;
  }
};

const handleClick = () => {
  emit('click', props.task);
};

const handleTimerToggle = () => {
  emit('timer-toggle', props.task);
};

const handleStatusChange = (statusId: string) => {
  emit('status-change', parseInt(statusId));
};

// Update timer duration every second if timer is running
onMounted(() => {
  if (props.task.timer_running) {
    updateTimerDuration();
    timerInterval.value = setInterval(updateTimerDuration, 1000);
  }
});

onUnmounted(() => {
  if (timerInterval.value) {
    clearInterval(timerInterval.value);
  }
});

// Watch for timer state changes
watch(() => props.task.timer_running, (isRunning) => {
  if (isRunning) {
    updateTimerDuration();
    timerInterval.value = setInterval(updateTimerDuration, 1000);
  } else {
    if (timerInterval.value) {
      clearInterval(timerInterval.value);
      timerInterval.value = null;
    }
    timerDuration.value = '';
  }
});
</script>

<style scoped>
/* Additional custom styles if needed */
</style>