<template>
	<div class="activity-filters space-x-2 flex items-center">
		<!-- Activity Type Filter -->
		<Select
			v-model="localFilters.type"
			@update:model-value="handleFilterChange"
		>
			<SelectTrigger class="w-32">
				<SelectValue
					:placeholder="$t('board.dashboard.activityFilters.types.allTypes')"
				/>
			</SelectTrigger>
			<SelectContent>
				<SelectItem value="all">
					{{ $t('board.dashboard.activityFilters.types.allTypes') }}
				</SelectItem>
				<SelectSeparator />
				<SelectItem value="task_created">
					{{ $t('board.dashboard.activityFilters.types.taskCreated') }}
				</SelectItem>
				<SelectItem value="task_updated">
					{{ $t('board.dashboard.activityFilters.types.taskUpdated') }}
				</SelectItem>
				<SelectItem value="task_completed">
					{{ $t('board.dashboard.activityFilters.types.taskCompleted') }}
				</SelectItem>
				<SelectItem value="task_deleted">
					{{ $t('board.dashboard.activityFilters.types.taskDeleted') }}
				</SelectItem>
				<SelectItem value="task_status_changed">
					{{ $t('board.dashboard.activityFilters.types.statusChanged') }}
				</SelectItem>
				<SelectItem value="task_assigned">
					{{ $t('board.dashboard.activityFilters.types.taskAssigned') }}
				</SelectItem>
				<SelectItem value="task_persona_assigned">
					{{ $t('board.dashboard.activityFilters.types.personaAssigned') }}
				</SelectItem>
				<SelectItem value="task_timer_started">
					{{ $t('board.dashboard.activityFilters.types.timerStarted') }}
				</SelectItem>
				<SelectItem value="task_timer_stopped">
					{{ $t('board.dashboard.activityFilters.types.timerStopped') }}
				</SelectItem>
				<SelectSeparator />
				<SelectItem value="comment_created">
					{{ $t('board.dashboard.activityFilters.types.commentAdded') }}
				</SelectItem>
				<SelectItem value="comment_updated">
					{{ $t('board.dashboard.activityFilters.types.commentUpdated') }}
				</SelectItem>
				<SelectItem value="comment_deleted">
					{{ $t('board.dashboard.activityFilters.types.commentDeleted') }}
				</SelectItem>
				<SelectSeparator />
				<SelectItem value="category_created">
					{{ $t('board.dashboard.activityFilters.types.categoryCreated') }}
				</SelectItem>
				<SelectItem value="category_updated">
					{{ $t('board.dashboard.activityFilters.types.categoryUpdated') }}
				</SelectItem>
				<SelectItem value="category_deleted">
					{{ $t('board.dashboard.activityFilters.types.categoryDeleted') }}
				</SelectItem>
				<SelectSeparator />
				<SelectItem value="file_uploaded">
					{{ $t('board.dashboard.activityFilters.types.fileUploaded') }}
				</SelectItem>
				<SelectItem value="file_deleted">
					{{ $t('board.dashboard.activityFilters.types.fileDeleted') }}
				</SelectItem>
				<SelectSeparator />
				<SelectItem value="member_joined">
					{{ $t('board.dashboard.activityFilters.types.memberJoined') }}
				</SelectItem>
				<SelectItem value="member_left">
					{{ $t('board.dashboard.activityFilters.types.memberLeft') }}
				</SelectItem>
				<SelectSeparator />
				<SelectItem value="routine_completed">
					{{ $t('board.dashboard.activityFilters.types.routineCompleted') }}
				</SelectItem>
			</SelectContent>
		</Select>

		<!-- User Filter -->
		<Select
			:model-value="localFilters.user_id?.toString()"
			@update:model-value="handleUserChange"
		>
			<SelectTrigger class="w-32">
				<SelectValue
					:placeholder="$t('board.dashboard.activityFilters.allUsers')"
				/>
			</SelectTrigger>
			<SelectContent>
				<SelectItem value="all">{{
					$t('board.dashboard.activityFilters.allUsers')
				}}</SelectItem>
				<SelectSeparator />
				<SelectItem
					v-for="user in workspaceUsers"
					:key="user.id"
					:value="user.id.toString()"
				>
					<span class="flex min-w-0 items-center gap-2">
						<UserAvatar
							:user-id="user.id"
							:name="user.name"
							:has-avatar="user.has_avatar ?? false"
							:size="20"
						/>
						<span class="truncate">{{ user.name }}</span>
					</span>
				</SelectItem>
			</SelectContent>
		</Select>

		<!-- Date Range Filter -->
		<Popover>
			<PopoverTrigger as-child>
				<button
					type="button"
					class="flex h-10 w-32 items-center justify-between rounded-md border border-input bg-background px-3 py-2 text-start text-sm"
				>
					<span class="flex items-center gap-2 truncate">
						<CalendarIcon class="h-4 w-4 shrink-0 opacity-70" />
						<span class="truncate">{{ $t(selectedDateRange.labelKey) }}</span>
					</span>
					<ChevronDown class="h-4 w-4 shrink-0 opacity-50" />
				</button>
			</PopoverTrigger>
			<PopoverContent class="w-auto p-0" align="start">
				<div class="space-y-2 p-3">
					<Button
						v-for="range in dateRanges"
						:key="range.value"
						variant="ghost"
						size="sm"
						class="w-full justify-start"
						@click="selectDateRange(range)"
					>
						{{ $t(range.labelKey) }}
					</Button>
				</div>
			</PopoverContent>
		</Popover>

		<!-- Clear Filters -->
		<button
			v-if="hasActiveFilters"
			type="button"
			@click="clearFilters"
			class="flex h-10 w-10 items-center justify-center rounded-md text-ink-subtle transition-colors hover:bg-surface-hover hover:text-ink"
			:aria-label="$t('board.dashboard.activityFilters.clearAria')"
		>
			<XMarkIcon class="h-4 w-4" />
		</button>
	</div>
</template>

<script setup lang="ts">
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import Button from '@/components/ui/button/Button.vue';
	import {
		Popover,
		PopoverContent,
		PopoverTrigger,
	} from '@/components/ui/popover';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectSeparator,
		SelectTrigger,
		SelectValue,
	} from '@/components/ui/select';
	import type { ActivityFilters as ActivityFiltersType } from '@/types/dashboard';
	import { CalendarIcon, XMarkIcon } from '@heroicons/vue/24/outline';
	import { ChevronDown } from 'lucide-vue-next';
	import { computed, ref, watch } from 'vue';

	interface Props {
		modelValue: ActivityFiltersType;
		workspaceUsers?: Array<{ id: number; name: string; has_avatar?: boolean }>;
	}

	interface DateRange {
		labelKey: string;
		value: string;
		from?: string;
		to?: string;
	}

	const props = withDefaults(defineProps<Props>(), {
		workspaceUsers: () => [],
	});

	const emit = defineEmits<{
		'update:model-value': [filters: ActivityFiltersType];
	}>();

	const localFilters = ref<ActivityFiltersType>({ ...props.modelValue });

	const dateRanges: DateRange[] = [
		{
			labelKey: 'board.dashboard.activityFilters.ranges.allTime',
			value: 'all',
		},
		{
			labelKey: 'board.dashboard.activityFilters.ranges.today',
			value: 'today',
		},
		{
			labelKey: 'board.dashboard.activityFilters.ranges.yesterday',
			value: 'yesterday',
		},
		{
			labelKey: 'board.dashboard.activityFilters.ranges.thisWeek',
			value: 'week',
		},
		{
			labelKey: 'board.dashboard.activityFilters.ranges.lastWeek',
			value: 'last_week',
		},
		{
			labelKey: 'board.dashboard.activityFilters.ranges.thisMonth',
			value: 'month',
		},
		{
			labelKey: 'board.dashboard.activityFilters.ranges.lastMonth',
			value: 'last_month',
		},
	];

	const selectedDateRange = ref<DateRange>(dateRanges[0]);

	const hasActiveFilters = computed(() => {
		return (
			localFilters.value.type !== 'all' ||
			localFilters.value.user_id !== 'all' ||
			selectedDateRange.value.value !== 'all'
		);
	});

	const selectDateRange = (range: DateRange) => {
		selectedDateRange.value = range;

		// Calculate date range based on selection
		const now = new Date();
		let from: string | undefined;
		let to: string | undefined;

		switch (range.value) {
			case 'today':
				from = new Date(
					now.getFullYear(),
					now.getMonth(),
					now.getDate(),
				).toISOString();
				to = new Date(
					now.getFullYear(),
					now.getMonth(),
					now.getDate() + 1,
				).toISOString();
				break;
			case 'yesterday':
				from = new Date(
					now.getFullYear(),
					now.getMonth(),
					now.getDate() - 1,
				).toISOString();
				to = new Date(
					now.getFullYear(),
					now.getMonth(),
					now.getDate(),
				).toISOString();
				break;
			case 'week':
				const startOfWeek = new Date(now);
				startOfWeek.setDate(now.getDate() - now.getDay());
				from = startOfWeek.toISOString();
				to = now.toISOString();
				break;
			case 'last_week':
				const lastWeekStart = new Date(now);
				lastWeekStart.setDate(now.getDate() - now.getDay() - 7);
				const lastWeekEnd = new Date(lastWeekStart);
				lastWeekEnd.setDate(lastWeekStart.getDate() + 7);
				from = lastWeekStart.toISOString();
				to = lastWeekEnd.toISOString();
				break;
			case 'month':
				from = new Date(now.getFullYear(), now.getMonth(), 1).toISOString();
				to = now.toISOString();
				break;
			case 'last_month':
				const lastMonth = new Date(now.getFullYear(), now.getMonth() - 1, 1);
				const lastMonthEnd = new Date(now.getFullYear(), now.getMonth(), 1);
				from = lastMonth.toISOString();
				to = lastMonthEnd.toISOString();
				break;
			default:
				from = undefined;
				to = undefined;
		}

		localFilters.value = {
			...localFilters.value,
			date_from: from,
			date_to: to,
		};

		handleFilterChange();
	};

	const clearFilters = () => {
		localFilters.value = {
			type: 'all',
			user_id: 'all',
			limit: 20,
		};
		selectedDateRange.value = dateRanges[0];
		handleFilterChange();
	};

	const handleUserChange = (value: unknown) => {
		// Select emits the item's string value; ActivityFilters.user_id is typed number | 'all' (shared type)
		localFilters.value.user_id = value as ActivityFiltersType['user_id'];
		handleFilterChange();
	};

	const handleFilterChange = () => {
		emit('update:model-value', { ...localFilters.value });
	};

	// Watch for external changes to modelValue
	watch(
		() => props.modelValue,
		(newValue) => {
			localFilters.value = { ...newValue };
		},
		{ deep: true },
	);
</script>

<style scoped>
	@reference '@/assets/styles/tailwind.css';

	.activity-filters {
		@apply flex-wrap gap-2;
	}

	@media (max-width: 640px) {
		.activity-filters {
			@apply flex-col items-stretch;
		}
	}
</style>
