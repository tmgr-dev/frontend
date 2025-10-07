<template>
  <div class="activity-filters flex items-center space-x-2">
    <!-- Activity Type Filter -->
    <Select v-model="localFilters.type" @update:model-value="handleFilterChange">
      <SelectTrigger class="w-32">
        <SelectValue placeholder="All Types" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="all">All Types</SelectItem>
        <SelectSeparator />
        <SelectItem value="task_created">Task Created</SelectItem>
        <SelectItem value="task_updated">Task Updated</SelectItem>
        <SelectItem value="task_completed">Task Completed</SelectItem>
        <SelectItem value="task_deleted">Task Deleted</SelectItem>
        <SelectItem value="task_status_changed">Status Changed</SelectItem>
        <SelectItem value="task_assigned">Task Assigned</SelectItem>
        <SelectItem value="task_timer_started">Timer Started</SelectItem>
        <SelectItem value="task_timer_stopped">Timer Stopped</SelectItem>
        <SelectSeparator />
        <SelectItem value="comment_created">Comment Added</SelectItem>
        <SelectItem value="comment_updated">Comment Updated</SelectItem>
        <SelectItem value="comment_deleted">Comment Deleted</SelectItem>
        <SelectSeparator />
        <SelectItem value="category_created">Category Created</SelectItem>
        <SelectItem value="category_updated">Category Updated</SelectItem>
        <SelectItem value="category_deleted">Category Deleted</SelectItem>
        <SelectSeparator />
        <SelectItem value="file_uploaded">File Uploaded</SelectItem>
        <SelectItem value="file_deleted">File Deleted</SelectItem>
        <SelectSeparator />
        <SelectItem value="member_joined">Member Joined</SelectItem>
        <SelectItem value="member_left">Member Left</SelectItem>
        <SelectSeparator />
        <SelectItem value="routine_completed">Routine Completed</SelectItem>
      </SelectContent>
    </Select>

    <!-- User Filter -->
    <Select v-model="localFilters.user_id" @update:model-value="handleFilterChange">
      <SelectTrigger class="w-32">
        <SelectValue placeholder="All Users" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="all">All Users</SelectItem>
        <SelectSeparator />
        <SelectItem 
          v-for="user in workspaceUsers" 
          :key="user.id" 
          :value="user.id.toString()"
        >
          {{ user.name }}
        </SelectItem>
      </SelectContent>
    </Select>

    <!-- Date Range Filter -->
    <Popover>
      <PopoverTrigger as-child>
        <Button variant="outline" size="sm" class="w-32">
          <CalendarIcon class="h-4 w-4 mr-2" />
          {{ dateRangeLabel }}
        </Button>
      </PopoverTrigger>
      <PopoverContent class="w-auto p-0" align="start">
        <div class="p-3 space-y-2">
          <Button
            v-for="range in dateRanges"
            :key="range.value"
            variant="ghost"
            size="sm"
            class="w-full justify-start"
            @click="selectDateRange(range)"
          >
            {{ range.label }}
          </Button>
        </div>
      </PopoverContent>
    </Popover>

    <!-- Clear Filters -->
    <Button
      v-if="hasActiveFilters"
      variant="ghost"
      size="sm"
      @click="clearFilters"
      class="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
    >
      <XMarkIcon class="h-4 w-4" />
    </Button>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import type { ActivityFilters as ActivityFiltersType } from '@/types/dashboard';
import Button from '@/components/ui/button/Button.vue';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { CalendarIcon, XMarkIcon } from '@heroicons/vue/24/outline';

interface Props {
  modelValue: ActivityFiltersType;
  workspaceUsers?: Array<{ id: number; name: string; }>;
}

interface DateRange {
  label: string;
  value: string;
  from?: string;
  to?: string;
}

const props = withDefaults(defineProps<Props>(), {
  workspaceUsers: () => []
});

const emit = defineEmits<{
  'update:model-value': [filters: ActivityFiltersType];
}>();

const localFilters = ref<ActivityFiltersType>({ ...props.modelValue });

const dateRanges: DateRange[] = [
  { label: 'All Time', value: 'all' },
  { label: 'Today', value: 'today' },
  { label: 'Yesterday', value: 'yesterday' },
  { label: 'This Week', value: 'week' },
  { label: 'Last Week', value: 'last_week' },
  { label: 'This Month', value: 'month' },
  { label: 'Last Month', value: 'last_month' }
];

const selectedDateRange = ref<DateRange>(dateRanges[0]);

const dateRangeLabel = computed(() => {
  return selectedDateRange.value.label;
});

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
      from = new Date(now.getFullYear(), now.getMonth(), now.getDate()).toISOString();
      to = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1).toISOString();
      break;
    case 'yesterday':
      from = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1).toISOString();
      to = new Date(now.getFullYear(), now.getMonth(), now.getDate()).toISOString();
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
      const lastMonthEnd = new Date(now.getFullYear(), now.getMonth(), 0);
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
    date_to: to
  };
  
  handleFilterChange();
};

const clearFilters = () => {
  localFilters.value = {
    type: 'all',
    user_id: 'all',
    limit: 20
  };
  selectedDateRange.value = dateRanges[0];
  handleFilterChange();
};

const handleFilterChange = () => {
  emit('update:model-value', { ...localFilters.value });
};

// Watch for external changes to modelValue
watch(() => props.modelValue, (newValue) => {
  localFilters.value = { ...newValue };
}, { deep: true });
</script>

<style scoped>
.activity-filters {
  @apply flex-wrap gap-2;
}

@media (max-width: 640px) {
  .activity-filters {
    @apply flex-col items-stretch;
  }
}
</style>
</template>
</invoke>