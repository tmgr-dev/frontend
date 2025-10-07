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
    <div class="flex items-center justify-between">
      <div class="flex items-center space-x-3">
        <div :class="iconClasses">
          <component :is="iconComponent" class="h-5 w-5" />
        </div>
        <div class="flex-1 min-w-0">
          <p class="text-sm font-medium text-gray-600 dark:text-gray-400 truncate">
            {{ title }}
          </p>
          <div class="flex items-baseline space-x-2">
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
        </div>
      </div>
    </div>
    
    <div v-if="description && !loading" class="mt-2">
      <p class="text-xs text-gray-500 dark:text-gray-400">
        {{ description }}
      </p>
    </div>
    
    <div v-if="loading" class="mt-2">
      <Skeleton class="h-3 w-24" />
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
    'bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4',
    'hover:shadow-md dark:hover:shadow-lg transition-all duration-200',
    'cursor-pointer focus:outline-none focus:ring-2 focus:ring-offset-2',
    'focus:ring-blue-500 dark:focus:ring-blue-400',
    'transform hover:scale-[1.02] active:scale-[0.98]',
    props.loading && 'animate-pulse'
  );
});

const iconClasses = computed(() => {
  const colorMap = {
    blue: 'bg-blue-100 text-blue-600 dark:bg-blue-900/20 dark:text-blue-400',
    green: 'bg-green-100 text-green-600 dark:bg-green-900/20 dark:text-green-400',
    orange: 'bg-orange-100 text-orange-600 dark:bg-orange-900/20 dark:text-orange-400',
    red: 'bg-red-100 text-red-600 dark:bg-red-900/20 dark:text-red-400',
    purple: 'bg-purple-100 text-purple-600 dark:bg-purple-900/20 dark:text-purple-400',
    indigo: 'bg-indigo-100 text-indigo-600 dark:bg-indigo-900/20 dark:text-indigo-400'
  };
  
  return cn(
    'flex items-center justify-center w-10 h-10 rounded-lg',
    colorMap[props.color]
  );
});

const valueClasses = computed(() => {
  return cn(
    'text-2xl font-bold text-gray-900 dark:text-white',
    props.loading && 'opacity-0'
  );
});

const trendClasses = computed(() => {
  if (!props.trend) return '';
  
  return cn(
    'flex items-center space-x-1',
    props.trend.isPositive 
      ? 'text-green-600 dark:text-green-400' 
      : 'text-red-600 dark:text-red-400'
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
</template>
</invoke>