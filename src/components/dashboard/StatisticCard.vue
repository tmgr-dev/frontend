<template>
  <div
    :class="cardClasses"
    @click="handleClick"
    @keydown.enter="handleClick"
    @keydown.space.prevent="handleClick"
    tabindex="0"
    role="button"
    :aria-label="`${title}: ${displayValue}. ${description || ''} Click to view details.`"
  >
    <div class="flex items-start gap-3">
      <div :class="iconClasses">
        <component :is="iconComponent" class="h-5 w-5" />
      </div>
      <div class="flex-1 min-w-0">
        <p class="text-2xs font-bold uppercase tracking-wide text-ink-subtle truncate">
          {{ title }}
        </p>
        <div class="mt-1 flex items-baseline gap-2">
          <p :class="valueClasses">
            <template v-if="loading">
              <Skeleton class="h-8 w-16" />
            </template>
            <template v-else>
              {{ displayValue }}
            </template>
          </p>
          <div v-if="trend && !loading" :class="trendClasses">
            <component :is="trendIcon" class="h-3 w-3" />
            <span class="text-xs font-medium">
              {{ Math.abs(trend.value) }}%
            </span>
          </div>
        </div>
        <p v-if="description && !loading" class="mt-2 text-xs text-ink-muted">
          {{ description }}
        </p>
        <div v-if="loading" class="mt-2">
          <Skeleton class="h-3 w-24" />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { cn } from '@/utils';
import Skeleton from '@/components/ui/skeleton/Skeleton.vue';

// Icon components (using simple SVG icons for now)
import {
  CheckIcon,
  ClockIcon,
  PlayIcon,
  UsersIcon,
  ExclamationTriangleIcon,
  ArrowTrendingUpIcon,
  ArrowTrendingDownIcon,
  CalendarIcon,
  ArrowPathIcon,
  ListBulletIcon
} from '@heroicons/vue/24/outline';

interface Props {
  title: string;
  value: number | string;
  icon: string;
  color: 'blue' | 'green' | 'orange' | 'red' | 'purple' | 'indigo';
  loading?: boolean;
  trend?: {
    value: number;
    isPositive: boolean;
    period: string;
  };
  description?: string;
}

const props = withDefaults(defineProps<Props>(), {
  loading: false
});

const emit = defineEmits<{
  click: [];
}>();

const displayValue = computed(() => {
  if (props.loading) return '';
  return props.value;
});

const iconComponent = computed(() => {
  const iconMap: Record<string, any> = {
    tasks: ListBulletIcon,
    check: CheckIcon,
    clock: ClockIcon,
    play: PlayIcon,
    users: UsersIcon,
    alert: ExclamationTriangleIcon,
    calendar: CalendarIcon,
    repeat: ArrowPathIcon
  };
  return iconMap[props.icon] || ListBulletIcon;
});

const trendIcon = computed(() => {
  return props.trend?.isPositive ? ArrowTrendingUpIcon : ArrowTrendingDownIcon;
});

const cardClasses = computed(() => {
  return cn(
    'rounded-card border border-line bg-surface p-4 min-h-28 shadow-tmgr-xs',
    'hover:shadow-tmgr-md hover:border-line-strong transition-all duration-150',
    'cursor-pointer focus:outline-none focus:shadow-tmgr-focus',
    props.loading && 'animate-pulse'
  );
});

const iconClasses = computed(() => {
  const colorMap = {
    blue: 'bg-status-progress-bg text-status-progress-fg',
    green: 'bg-status-done-bg text-status-done-fg',
    orange: 'bg-status-fix-bg text-status-fix-fg',
    red: 'bg-status-fix-bg text-status-fix-fg',
    purple: 'bg-status-testing-bg text-status-testing-fg',
    indigo: 'bg-brand-bg text-brand-fg',
  };

  return cn(
    'flex items-center justify-center w-10 h-10 rounded-card shrink-0',
    colorMap[props.color]
  );
});

const valueClasses = computed(() => {
  return cn(
    'text-2xl font-semibold tabular-nums text-ink',
    props.loading && 'opacity-0'
  );
});

const trendClasses = computed(() => {
  if (!props.trend) return '';

  return cn(
    'flex items-center gap-1',
    props.trend.isPositive
      ? 'text-status-done-fg'
      : 'text-status-fix-fg'
  );
});

const handleClick = () => {
  if (!props.loading) {
    emit('click');
  }
};
</script>

<style scoped>
/* Additional custom styles if needed */
</style>