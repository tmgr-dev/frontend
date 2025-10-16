<template>
  <div class="statistics-grid">
    <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4 mb-6">
      <StatisticCard
        v-for="stat in statisticCards"
        :key="stat.key"
        :title="stat.title"
        :value="stat.value"
        :icon="stat.icon"
        :color="stat.color"
        :loading="loading"
        :trend="stat.trend"
        :description="stat.description"
        @click="handleCardClick(stat.filter)"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { useRouter } from 'vue-router';
import type { DashboardStatistics } from '@/types/dashboard';
import StatisticCard from './StatisticCard.vue';

interface Props {
  statistics: DashboardStatistics;
  loading: boolean;
}

interface StatisticCardData {
  key: string;
  title: string;
  value: number | string;
  icon: string;
  color: 'blue' | 'green' | 'orange' | 'red' | 'purple' | 'indigo';
  trend?: {
    value: number;
    isPositive: boolean;
    period: string;
  };
  description?: string;
  filter?: Record<string, any>;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'navigate-to-filtered': [filter: Record<string, any>];
}>();

const router = useRouter();

const formatTime = (seconds: number): string => {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
};

const formatPercentage = (value: number): string => {
  return `${Math.round(value * 100)}%`;
};

const statisticCards = computed<StatisticCardData[]>(() => [
  {
    key: 'total_tasks',
    title: 'Total Tasks',
    value: props.statistics.total_tasks || 0,
    icon: 'tasks',
    color: 'blue',
    description: 'All tasks in workspace',
    filter: { status: 'all' }
  },
  {
    key: 'active_tasks',
    title: 'Active Tasks',
    value: props.statistics.active_tasks || 0,
    icon: 'play',
    color: 'green',
    description: 'Currently in progress',
    filter: { status: 'active' }
  },
  {
    key: 'completed_today',
    title: 'Completed Today',
    value: props.statistics.completed_today || 0,
    icon: 'check',
    color: 'green',
    description: 'Tasks finished today',
    filter: { status: 'completed', period: 'today' }
  },
  {
    key: 'completed_week',
    title: 'Completed This Week',
    value: props.statistics.completed_week || 0,
    icon: 'calendar',
    color: 'indigo',
    description: 'Tasks finished this week',
    filter: { status: 'completed', period: 'week' }
  },
  {
    key: 'time_today',
    title: 'Time Today',
    value: formatTime(props.statistics.time_today || 0),
    icon: 'clock',
    color: 'orange',
    description: 'Time tracked today',
    filter: { view: 'time_tracking', period: 'today' }
  },
  {
    key: 'time_week',
    title: 'Time This Week',
    value: formatTime(props.statistics.time_week || 0),
    icon: 'clock',
    color: 'purple',
    description: 'Time tracked this week',
    filter: { view: 'time_tracking', period: 'week' }
  },
  {
    key: 'team_members',
    title: 'Team Members',
    value: props.statistics.team_members || 0,
    icon: 'users',
    color: 'blue',
    description: 'Active workspace members',
    filter: { view: 'team' }
  },
  {
    key: 'overdue_tasks',
    title: 'Overdue Tasks',
    value: props.statistics.overdue_tasks || 0,
    icon: 'alert',
    color: 'red',
    description: 'Tasks past due date',
    filter: { status: 'overdue' }
  },
  {
    key: 'daily_routine_completion',
    title: 'Daily Routine',
    value: formatPercentage(props.statistics.daily_routine_completion_rate || 0),
    icon: 'repeat',
    color: 'green',
    description: 'Completion rate',
    filter: { view: 'daily_routine' }
  }
]);

const handleCardClick = (filter?: Record<string, any>) => {
  // if (!filter) return;
  
  // emit('navigate-to-filtered', filter);
  
  // // Navigate to appropriate route based on filter
  // if (filter.view === 'team') {
  //   router.push('/team');
  // } else if (filter.view === 'time_tracking') {
  //   router.push('/stats');
  // } else if (filter.view === 'daily_routine') {
  //   router.push('/daily-routine');
  // } else {
  //   // Navigate to tasks list with filters
  //   const query: Record<string, string> = {};
  //   if (filter.status && filter.status !== 'all') {
  //     query.status = filter.status;
  //   }
  //   if (filter.period) {
  //     query.period = filter.period;
  //   }
  //   router.push({ path: '/tasks', query });
  // }
};
</script>

<style scoped>
.statistics-grid {
  @apply w-full;
}

/* 1 column when width < 700px */
@media (max-width: 699px) {
  .statistics-grid .grid {
    grid-template-columns: repeat(1, 1fr);
  }
}

/* 2 columns when width < 980px */
@media (min-width: 700px) and (max-width: 979px) {
  .statistics-grid .grid {
    grid-template-columns: repeat(2, 1fr);
  }
}

/* 3 columns when width < 1290px */
@media (min-width: 980px) and (max-width: 1289px) {
  .statistics-grid .grid {
    grid-template-columns: repeat(3, 1fr);
  }
}

/* 4 columns on screens >= 1290px */
@media (min-width: 1290px) {
  .statistics-grid .grid {
    grid-template-columns: repeat(4, 1fr);
  }
}
</style>