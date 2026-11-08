<template>
	<!-- CHANGES: Added min-h-32 to prevent CLS -->
	<div class="statistics-grid min-h-32">
		<div class="mb-6 grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-4">
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
				@click="handleCardClick()"
			/>
		</div>
	</div>
</template>

<script setup lang="ts">
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import { t } from '@/i18n';
	import type { DashboardStatistics } from '@/types/dashboard';
	import { computed } from 'vue';
	import StatisticCard from './StatisticCard.vue';

	interface Props {
		statistics: Partial<DashboardStatistics>;
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
	defineEmits<{
		'navigate-to-filtered': [filter: Record<string, any>];
	}>();

	const formatTime = (seconds: number): string => {
		if (seconds < 60) return `${seconds}s`;
		if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
		return `${Math.floor(seconds / 3600)}h ${Math.floor(
			(seconds % 3600) / 60,
		)}m`;
	};

	const formatPercentage = (value: number): string => {
		return `${Math.round(value * 100)}%`;
	};

	const { showSurface } = useFeatureToggles();

	const statisticCards = computed<StatisticCardData[]>(() =>
		(
			[
				{
					key: 'total_tasks',
					title: t('board.dashboard.statisticsGrid.totalTasks.title'),
					value: props.statistics.total_tasks || 0,
					icon: 'tasks',
					color: 'blue',
					description: t(
						'board.dashboard.statisticsGrid.totalTasks.description',
					),
					filter: { status: 'all' },
				},
				{
					key: 'active_tasks',
					title: t('board.dashboard.statisticsGrid.activeTasks.title'),
					value: props.statistics.active_tasks || 0,
					icon: 'play',
					color: 'green',
					description: t(
						'board.dashboard.statisticsGrid.activeTasks.description',
					),
					filter: { status: 'active' },
				},
				{
					key: 'completed_today',
					title: t('board.dashboard.statisticsGrid.completedToday.title'),
					value: props.statistics.completed_today || 0,
					icon: 'check',
					color: 'green',
					description: t(
						'board.dashboard.statisticsGrid.completedToday.description',
					),
					filter: { status: 'completed', period: 'today' },
				},
				{
					key: 'completed_week',
					title: t('board.dashboard.statisticsGrid.completedWeek.title'),
					value: props.statistics.completed_week || 0,
					icon: 'calendar',
					color: 'indigo',
					description: t(
						'board.dashboard.statisticsGrid.completedWeek.description',
					),
					filter: { status: 'completed', period: 'week' },
				},
				{
					key: 'time_today',
					title: t('board.dashboard.statisticsGrid.timeToday.title'),
					value: formatTime(props.statistics.time_today || 0),
					icon: 'clock',
					color: 'orange',
					description: t(
						'board.dashboard.statisticsGrid.timeToday.description',
					),
					filter: { view: 'time_tracking', period: 'today' },
				},
				{
					key: 'time_week',
					title: t('board.dashboard.statisticsGrid.timeWeek.title'),
					value: formatTime(props.statistics.time_week || 0),
					icon: 'clock',
					color: 'purple',
					description: t('board.dashboard.statisticsGrid.timeWeek.description'),
					filter: { view: 'time_tracking', period: 'week' },
				},
				{
					key: 'team_members',
					title: t('board.dashboard.statisticsGrid.teamMembers.title'),
					value: props.statistics.team_members || 0,
					icon: 'users',
					color: 'blue',
					description: t(
						'board.dashboard.statisticsGrid.teamMembers.description',
					),
					filter: { view: 'team' },
				},
				{
					key: 'overdue_tasks',
					title: t('board.dashboard.statisticsGrid.overdueTasks.title'),
					value: props.statistics.overdue_tasks || 0,
					icon: 'alert',
					color: 'red',
					description: t(
						'board.dashboard.statisticsGrid.overdueTasks.description',
					),
					filter: { status: 'overdue' },
				},
				{
					key: 'daily_routine_completion',
					title: t('board.dashboard.statisticsGrid.dailyRoutine.title'),
					value: formatPercentage(
						props.statistics.daily_routine_completion_rate || 0,
					),
					icon: 'repeat',
					color: 'green',
					description: t(
						'board.dashboard.statisticsGrid.dailyRoutine.description',
					),
					filter: { view: 'daily_routine' },
				},
			] satisfies StatisticCardData[]
		).filter(
			(card) =>
				(!['time_today', 'time_week'].includes(card.key) ||
					showSurface('timer.stats')) &&
				(card.key !== 'daily_routine_completion' ||
					showSurface('routines.stats')),
		),
	);

	const handleCardClick = () => {
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
	@reference '@/assets/styles/tailwind.css';

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
