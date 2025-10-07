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
    <div class="flex items-center justify-between">
      <div class="flex items-center space-x-3">
        <!-- Member Avatar with Status -->
        <div class="relative">
          <img
            v-if="member.avatar"
            :src="member.avatar"
            :alt="member.name"
            class="w-10 h-10 rounded-full"
          />
          <div
            v-else
            class="w-10 h-10 rounded-full bg-gray-300 dark:bg-gray-600 flex items-center justify-center"
          >
            <span class="text-sm font-medium text-gray-600 dark:text-gray-300">
              {{ memberInitials }}
            </span>
          </div>
          
          <!-- Online Status Indicator -->
          <div 
            :class="statusIndicatorClasses"
            :title="member.is_online ? 'Online' : 'Offline'"
          ></div>
        </div>
        
        <!-- Member Info -->
        <div class="flex-1 min-w-0">
          <div class="flex items-center space-x-2">
            <h4 class="text-sm font-medium text-gray-900 dark:text-white truncate">
              {{ member.name }}
            </h4>
            <div v-if="member.current_task?.timer_running" class="flex items-center">
              <div class="w-2 h-2 bg-green-500 rounded-full animate-pulse"></div>
              <span class="sr-only">Timer running</span>
            </div>
          </div>
          
          <!-- Current Task or Last Activity -->
          <div class="text-xs text-gray-500 dark:text-gray-400">
            <template v-if="member.current_task">
              <div class="flex items-center space-x-1">
                <PlayIcon v-if="member.current_task.timer_running" class="h-3 w-3 text-green-500" />
                <ClockIcon v-else class="h-3 w-3" />
                <button
                  @click.stop="handleTaskClick(member.current_task.id)"
                  class="hover:text-blue-600 dark:hover:text-blue-400 truncate max-w-32"
                  :title="member.current_task.title"
                >
                  {{ member.current_task.title }}
                </button>
              </div>
            </template>
            <template v-else-if="member.last_activity_at">
              <div class="flex items-center space-x-1">
                <ClockIcon class="h-3 w-3" />
                <time 
                  :datetime="member.last_activity_at"
                  :title="formattedLastActivity"
                >
                  Last active {{ lastActivityHuman }}
                </time>
              </div>
            </template>
            <template v-else>
              <span>No recent activity</span>
            </template>
          </div>
        </div>
      </div>
      
      <!-- Timer Duration (if running) -->
      <div v-if="member.current_task?.timer_running" class="text-right">
        <div class="text-xs font-mono text-green-600 dark:text-green-400">
          {{ timerDuration }}
        </div>
        <div class="text-xs text-gray-500 dark:text-gray-400">
          Running
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted, watch } from 'vue';
import type { TeamMemberStatus } from '@/types/dashboard';
import { cn } from '@/utils';
import { PlayIcon, ClockIcon } from '@heroicons/vue/24/outline';

interface Props {
  member: TeamMemberStatus;
}

const props = defineProps<Props>();

const emit = defineEmits<{
  click: [member: TeamMemberStatus];
  'task-click': [taskId: number];
}>();

const timerInterval = ref<NodeJS.Timeout | null>(null);
const timerDuration = ref<string>('');

const memberInitials = computed(() => {
  return props.member.name
    .split(' ')
    .map(name => name.charAt(0))
    .join('')
    .toUpperCase()
    .slice(0, 2);
});

const formattedLastActivity = computed(() => {
  if (!props.member.last_activity_at) return '';
  return new Date(props.member.last_activity_at).toLocaleString();
});

const lastActivityHuman = computed(() => {
  if (!props.member.last_activity_at) return '';
  
  const now = new Date();
  const lastActivity = new Date(props.member.last_activity_at);
  const diffInMinutes = Math.floor((now.getTime() - lastActivity.getTime()) / (1000 * 60));
  
  if (diffInMinutes < 1) return 'just now';
  if (diffInMinutes < 60) return `${diffInMinutes}m ago`;
  
  const diffInHours = Math.floor(diffInMinutes / 60);
  if (diffInHours < 24) return `${diffInHours}h ago`;
  
  const diffInDays = Math.floor(diffInHours / 24);
  return `${diffInDays}d ago`;
});

const statusIndicatorClasses = computed(() => {
  return cn(
    'absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full border-2 border-white dark:border-gray-800',
    props.member.is_online 
      ? 'bg-green-500' 
      : 'bg-gray-400 dark:bg-gray-600'
  );
});

const itemClasses = computed(() => {
  return cn(
    'p-3 rounded-lg border border-gray-200 dark:border-gray-700',
    'hover:bg-gray-50 dark:hover:bg-gray-700/50 transition-colors duration-150',
    'cursor-pointer focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400',
    'focus:ring-offset-2 dark:focus:ring-offset-gray-800',
    props.member.is_online && 'border-green-200 dark:border-green-800'
  );
});

const accessibilityLabel = computed(() => {
  const parts = [
    `Team member: ${props.member.name}`,
    props.member.is_online ? 'Online' : 'Offline'
  ];
  
  if (props.member.current_task) {
    parts.push(`Working on: ${props.member.current_task.title}`);
    if (props.member.current_task.timer_running) {
      parts.push('Timer running');
    }
  } else if (props.member.last_activity_at) {
    parts.push(`Last active: ${lastActivityHuman.value}`);
  }
  
  return parts.join(', ');
});

const updateTimerDuration = () => {
  if (!props.member.current_task?.timer_running || !props.member.current_task?.timer_started_at) {
    timerDuration.value = '';
    return;
  }
  
  const startTime = new Date(props.member.current_task.timer_started_at).getTime();
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
  emit('click', props.member);
};

const handleTaskClick = (taskId: number) => {
  emit('task-click', taskId);
};

// Update timer duration every second if timer is running
onMounted(() => {
  if (props.member.current_task?.timer_running) {
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
watch(() => props.member.current_task?.timer_running, (isRunning) => {
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
</template>
</invoke>