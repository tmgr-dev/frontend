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
    <div class="flex items-center gap-4">
      <!-- Avatar + presence dot -->
      <div class="relative shrink-0">
        <img v-if="member.avatar" :src="member.avatar" :alt="member.name" class="w-10 h-10 rounded-full object-cover" loading="lazy" />
        <div v-else class="w-10 h-10 rounded-full bg-gray-300 dark:bg-gray-600 flex items-center justify-center">
          <span class="text-sm font-medium text-gray-600 dark:text-gray-300">{{ memberInitials }}</span>
        </div>
        <div :class="statusIndicatorClasses" :title="member.is_online ? 'Active in the last 5 minutes' : 'Inactive'"></div>
      </div>

      <!-- Name / role / current or last task -->
      <div class="flex-1 min-w-0">
        <div class="flex items-center gap-2">
          <h4 class="text-sm font-medium text-ink truncate">{{ member.name }}</h4>
          <span class="text-2xs uppercase tracking-wide px-1.5 py-0.5 rounded bg-surface-sunken text-ink-subtle">{{ member.role }}</span>
        </div>
        <div class="text-xs text-ink-subtle mt-0.5 flex items-center gap-1 min-w-0">
          <template v-if="member.current_task">
            <PlayIcon class="h-3 w-3 text-status-done shrink-0" />
            <button @click.stop="handleTaskClick(member.current_task.id)" class="hover:text-brand-fg truncate" :title="member.current_task.title">
              {{ member.current_task.title }}
            </button>
            <span v-if="timerDuration" class="font-mono text-status-done-fg shrink-0">· {{ timerDuration }}</span>
          </template>
          <template v-else-if="member.last_activity_at">
            <ClockIcon class="h-3 w-3 shrink-0" />
            <time :datetime="member.last_activity_at" :title="formattedLastActivity">Last active {{ lastActivityHuman }}</time>
            <template v-if="member.last_task">
              <span class="shrink-0">· last touched</span>
              <button @click.stop="handleTaskClick(member.last_task.id)" class="hover:text-brand-fg truncate" :title="member.last_task.title">
                {{ member.last_task.title }}
              </button>
            </template>
          </template>
          <template v-else><span>No recent activity</span></template>
        </div>
      </div>

      <!-- Sparkline -->
      <div class="hidden md:flex items-end gap-0.5 h-6 shrink-0" :title="`Activity, last ${dailyActivityPoints.length} day(s)`" aria-hidden="true">
        <span
          v-for="(h, i) in bars"
          :key="dailyActivityPoints[i]?.date ?? i"
          class="w-1.5 rounded-sm"
          :class="h > 0 ? 'bg-brand' : 'bg-surface-sunken'"
          :style="{ height: `${Math.max(h, 2)}px` }"
        ></span>
      </div>

      <!-- Stats -->
      <dl class="grid grid-cols-5 gap-x-4 text-right shrink-0">
        <div><dt class="sr-only">Tracked</dt><dd class="text-sm font-semibold tabular-nums text-status-done-fg">{{ tracked }}</dd><dd class="text-2xs uppercase tracking-wide text-ink-faint">tracked</dd></div>
        <div><dt class="sr-only">Done</dt><dd class="text-sm font-semibold tabular-nums text-brand-fg">{{ member.done_count }}</dd><dd class="text-2xs uppercase tracking-wide text-ink-faint">done</dd></div>
        <div><dt class="sr-only">Active tasks</dt><dd class="text-sm font-semibold tabular-nums text-status-progress-fg">{{ member.active_tasks }}</dd><dd class="text-2xs uppercase tracking-wide text-ink-faint">active</dd></div>
        <div><dt class="sr-only">Comments</dt><dd class="text-sm font-semibold tabular-nums text-ink">{{ member.comments_count }}</dd><dd class="text-2xs uppercase tracking-wide text-ink-faint">comments</dd></div>
        <div><dt class="sr-only">Streak</dt><dd class="text-sm font-semibold tabular-nums text-ink">{{ member.streak }}</dd><dd class="text-2xs uppercase tracking-wide text-ink-faint">streak</dd></div>
      </dl>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted, watch } from 'vue';
import type { TeamMemberStatus } from '@/types/dashboard';
import { cn } from '@/utils';
import { PlayIcon, ClockIcon } from '@heroicons/vue/24/outline';
import { formatTrackedSeconds, sparklineBars } from '@/utils/dashboard/teamActivityFormat';

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
    'p-3 rounded-card border border-line bg-surface',
    'hover:bg-surface-hover hover:border-line-strong transition-colors duration-150',
    'cursor-pointer focus:outline-none focus:shadow-tmgr-focus',
    props.member.is_online && 'border-status-done/40'
  );
});

const tracked = computed(() => formatTrackedSeconds(props.member.tracked_seconds));
const dailyActivityPoints = computed(() => props.member.daily_activity ?? []);
const bars = computed(() => sparklineBars(dailyActivityPoints.value, 24, 2));

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

  parts.push(`${tracked.value} tracked, ${props.member.done_count} done, ${props.member.active_tasks} active, ${props.member.comments_count} comments, streak ${props.member.streak}`);

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
