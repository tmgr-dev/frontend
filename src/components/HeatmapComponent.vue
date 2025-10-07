<script setup lang="ts">
import { ref, onMounted, computed, watch } from 'vue';
import { getUserHeatmap } from '@/actions/tmgr/dashboard';
import type { HeatmapData, ContributionData, ActionError } from '@/types/dashboard';

// Props
interface Props {
  workspaceId: number;
  userId?: number;
  year?: number;
  loading?: boolean;
  error?: ActionError | null;
}

const props = withDefaults(defineProps<Props>(), {
  year: () => new Date().getFullYear(),
  loading: false,
  error: null
});

// Emits
const emit = defineEmits<{
  'update:loading': [loading: boolean];
  'update:error': [error: ActionError | null];
  'date-click': [date: string, count: number];
  'retry': [];
}>();

// State
const heatmapData = ref<HeatmapData | null>(null);
const internalLoading = ref(false);
const internalError = ref<ActionError | null>(null);

// Computed loading and error states
const isLoading = computed(() => props.loading || internalLoading.value);
const currentError = computed(() => props.error || internalError.value);

// Days of week for y-axis
const daysOfWeek = ['Mon', 'Wed', 'Fri'];

// Months for x-axis - starting from current year's beginning
const months = computed(() => {
  const monthNames = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  const currentMonth = new Date().getMonth();
  const currentYear = new Date().getFullYear();
  
  if (props.year === currentYear) {
    // For current year, start from January to current month + 1
    return monthNames.slice(0, currentMonth + 2);
  } else {
    // For other years, show all months
    return monthNames;
  }
});

// Activity level descriptions
const activityLevels = [
  'No activity',
  'Low activity', 
  'Medium activity',
  'High activity',
  'Very high activity'
];

// Load heatmap data from API
const loadHeatmapData = async () => {
  if (isLoading.value) return;
  
  try {
    internalLoading.value = true;
    internalError.value = null;
    emit('update:loading', true);
    emit('update:error', null);
    
    const result = await getUserHeatmap(props.workspaceId, {
      year: props.year,
      user_id: props.userId,
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      include_weekends: true
    });
    
    if (result.success && result.data) {
      heatmapData.value = result.data;
    } else if (result.error) {
      internalError.value = result.error;
      emit('update:error', result.error);
    }
  } catch (error) {
    const actionError: ActionError = {
      message: 'Failed to load heatmap data',
      type: 'network',
      timestamp: new Date().toISOString(),
      recoverable: true,
      details: error
    };
    internalError.value = actionError;
    emit('update:error', actionError);
  } finally {
    internalLoading.value = false;
    emit('update:loading', false);
  }
};

// Generate 365 days of dates for the specified year
const generateYearDates = (year: number): string[] => {
  const dates: string[] = [];
  const startDate = new Date(year, 0, 1); // January 1st of the year
  const endDate = new Date(year, 11, 31); // December 31st of the year
  
  const currentDate = new Date(startDate);
  while (currentDate <= endDate) {
    dates.push(currentDate.toISOString().split('T')[0]);
    currentDate.setDate(currentDate.getDate() + 1);
  }
  
  return dates;
};

// Transform API data into a map for quick lookup
const contributionMap = computed(() => {
  if (!heatmapData.value?.contributions) return new Map<string, ContributionData>();
  
  const map = new Map<string, ContributionData>();
  heatmapData.value.contributions.forEach(contribution => {
    map.set(contribution.date, contribution);
  });
  return map;
});

// Generate complete year data with API data merged in
const yearContributions = computed(() => {
  const yearDates = generateYearDates(props.year);
  return yearDates.map(date => {
    const contribution = contributionMap.value.get(date);
    return {
      date,
      count: contribution?.count || 0,
      level: contribution?.level || 0
    };
  });
});

// Function to determine activity level (0-4) for CSS class
const getActivityLevel = (count: number) => {
  if (count === 0) return 0;
  if (count < 3) return 1;
  if (count < 6) return 2;
  if (count < 10) return 3;
  return 4;
};

// Format date for tooltip with proper timezone handling
const formatDate = (dateStr: string) => {
  const date = new Date(dateStr + 'T00:00:00');
  return date.toLocaleDateString('en-US', { 
    weekday: 'long',
    year: 'numeric', 
    month: 'long', 
    day: 'numeric',
    timeZone: 'UTC'
  });
};

// Tooltip state
const tooltip = ref({
  visible: false,
  x: 0,
  y: 0,
  date: '',
  count: 0,
  level: 0
});

// Handle date cell hover
const handleCellHover = (event: MouseEvent, cell: GridCell) => {
  if (cell.isEmpty || !cell.date) return;
  
  const rect = (event.target as HTMLElement).getBoundingClientRect();
  tooltip.value = {
    visible: true,
    x: rect.left + rect.width / 2,
    y: rect.top - 10,
    date: cell.date,
    count: cell.value || 0,
    level: cell.level
  };
};

// Handle mouse leave
const handleCellLeave = () => {
  tooltip.value.visible = false;
};

// Handle date cell click
const handleDateClick = (date: string, count: number) => {
  emit('date-click', date, count);
};

// Handle retry action
const handleRetry = () => {
  emit('retry');
  loadHeatmapData();
};

// Format tooltip content
const getTooltipContent = (date: string, count: number, level: number) => {
  const formattedDate = formatDate(date);
  const activityDescription = activityLevels[level] || 'No activity';
  
  if (count === 0) {
    return `${formattedDate}\nNo contributions`;
  } else if (count === 1) {
    return `${formattedDate}\n1 contribution\n${activityDescription}`;
  } else {
    return `${formattedDate}\n${count} contributions\n${activityDescription}`;
  }
};

// Get contribution level description
const getLevelDescription = (level: number) => {
  return activityLevels[level] || 'No activity';
};

// Define the type for a cell
interface GridCell {
  value: number | null;
  date: string | null;
  level: number;
  isEmpty: boolean;
}

// Generate 7x53 grid (53 weeks x 7 days) with real data
const calendarGrid = computed(() => {
  const grid: GridCell[][] = [];
  
  // Initialize with empty grid
  for (let row = 0; row < 7; row++) {
    const rowData: GridCell[] = [];
    for (let col = 0; col < 53; col++) {
      rowData.push({ 
        value: null, 
        date: null, 
        level: 0,
        isEmpty: true 
      });
    }
    grid.push(rowData);
  }
  
  // Fill grid with real contribution data
  if (yearContributions.value.length > 0) {
    const startDate = new Date(props.year, 0, 1);
    let startDayOfWeek = startDate.getDay(); // 0 = Sunday, 1 = Monday, etc.
    startDayOfWeek = startDayOfWeek === 0 ? 6 : startDayOfWeek - 1; // Adjust to 0 = Monday, 6 = Sunday
    
    let week = 0;
    let dayOfWeek = startDayOfWeek;
    
    yearContributions.value.forEach((contribution, index) => {
      if (week < 53 && dayOfWeek < 7) {
        grid[dayOfWeek][week] = {
          value: contribution.count,
          date: contribution.date,
          level: contribution.level || getActivityLevel(contribution.count),
          isEmpty: false
        };
      }
      
      dayOfWeek++;
      if (dayOfWeek >= 7) {
        dayOfWeek = 0;
        week++;
      }
    });
  }
  
  return grid;
});

// Computed values for display
const totalContributions = computed(() => {
  return heatmapData.value?.total_contributions || 0;
});

const streakInfo = computed(() => {
  return heatmapData.value?.streak || { current: 0, longest: 0 };
});

// Smooth data update with transition
const updateDataWithTransition = async () => {
  // Add fade-out class to current content
  const calendarGrid = document.querySelector('.calendar-grid');
  if (calendarGrid) {
    calendarGrid.classList.add('opacity-50');
  }
  
  // Load new data
  await loadHeatmapData();
  
  // Remove fade-out class and trigger fade-in
  setTimeout(() => {
    if (calendarGrid) {
      calendarGrid.classList.remove('opacity-50');
    }
  }, 150);
};

// Watch for prop changes and reload data with smooth transition
watch([() => props.workspaceId, () => props.userId, () => props.year], () => {
  updateDataWithTransition();
}, { immediate: false });

onMounted(() => {
  loadHeatmapData();
});
</script>

<template>
  <div class="activity-heatmap-container">
    <!-- Loading State -->
    <div v-if="isLoading && !heatmapData" class="loading-state">
      <div class="contribution-header">
        <div class="skeleton-stats">
          <div class="skeleton-text w-64 h-6 mb-2"></div>
          <div class="skeleton-text w-48 h-4"></div>
        </div>
        <div class="skeleton-spinner"></div>
      </div>
      <div class="calendar-container">
        <div class="skeleton-calendar">
          <div class="skeleton-months">
            <div class="skeleton-month-spacer"></div>
            <div v-for="i in 12" :key="i" class="skeleton-month" :style="{ animationDelay: `${i * 0.1}s` }"></div>
          </div>
          <div class="skeleton-body">
            <div class="skeleton-day-labels">
              <div v-for="i in 3" :key="i" class="skeleton-day-label"></div>
            </div>
            <div class="skeleton-grid">
              <div v-for="row in 7" :key="row" class="skeleton-row">
                <div 
                  v-for="col in 53" 
                  :key="col" 
                  class="skeleton-cell"
                  :style="{ animationDelay: `${(row * 53 + col) * 0.01}s` }"
                ></div>
              </div>
            </div>
          </div>
          <div class="skeleton-footer">
            <div class="skeleton-activity-info">
              <div class="skeleton-text w-8 h-3"></div>
              <div class="skeleton-level-boxes">
                <div v-for="i in 5" :key="i" class="skeleton-level-box"></div>
              </div>
              <div class="skeleton-text w-8 h-3"></div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Error State -->
    <div v-else-if="currentError" class="error-state">
      <div class="error-content">
        <div class="error-icon">⚠️</div>
        <h3 class="error-title">Failed to load activity data</h3>
        <p class="error-message">{{ currentError.message }}</p>
        <button 
          v-if="currentError.recoverable"
          @click="handleRetry"
          class="retry-button"
          :disabled="isLoading"
        >
          {{ isLoading ? 'Retrying...' : 'Try Again' }}
        </button>
      </div>
    </div>

    <!-- Main Content -->
    <div v-else class="heatmap-content">
      <div class="contribution-header">
        <div class="contribution-stats">
          <div class="font-medium text-xl">
            {{ totalContributions }} contributions in {{ props.year }}
          </div>
          <div v-if="streakInfo.current > 0" class="streak-info">
            Current streak: {{ streakInfo.current }} days
            <span v-if="streakInfo.longest > streakInfo.current" class="longest-streak">
              (Longest: {{ streakInfo.longest }} days)
            </span>
          </div>
        </div>
        <div v-if="isLoading" class="loading-indicator">
          <div class="spinner"></div>
        </div>
      </div>
      
      <div class="calendar-container">
        <div class="months-header">
          <div class="month-names">
            <div class="empty-cell"></div>
            <div v-for="month in months" :key="month" class="month-label">{{ month }}</div>
          </div>
        </div>
        
        <div class="calendar-body">
          <div class="day-labels">
            <div v-for="day in daysOfWeek" :key="day" class="day-label">{{ day }}</div>
          </div>
          
          <div class="calendar-grid">
            <div v-for="(row, rowIndex) in calendarGrid" :key="`row-${rowIndex}`" class="calendar-row">
              <div 
                v-for="(cell, colIndex) in row" 
                :key="`cell-${rowIndex}-${colIndex}`"
                class="calendar-cell" 
                :class="[
                  !cell.isEmpty ? `activity-level-${cell.level}` : 'empty-cell',
                  { 
                    'clickable': !cell.isEmpty,
                    'has-activity': !cell.isEmpty && cell.value && cell.value > 0
                  }
                ]"
                :tabindex="!cell.isEmpty ? 0 : -1"
                :aria-label="cell.date && !cell.isEmpty ? getTooltipContent(cell.date, cell.value || 0, cell.level) : 'No activity'"
                @mouseenter="handleCellHover($event, cell)"
                @mouseleave="handleCellLeave"
                @click="cell.date && !cell.isEmpty ? handleDateClick(cell.date, cell.value || 0) : null"
                @keydown.enter="cell.date && !cell.isEmpty ? handleDateClick(cell.date, cell.value || 0) : null"
                @keydown.space.prevent="cell.date && !cell.isEmpty ? handleDateClick(cell.date, cell.value || 0) : null"
              ></div>
            </div>
          </div>
        </div>
        
        <div class="calendar-footer">
          <div class="activity-info">
            <span>Less</span>
            <div class="activity-level-boxes">
              <div class="activity-level-box activity-level-0"></div>
              <div class="activity-level-box activity-level-1"></div>
              <div class="activity-level-box activity-level-2"></div>
              <div class="activity-level-box activity-level-3"></div>
              <div class="activity-level-box activity-level-4"></div>
            </div>
            <span>More</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Custom Tooltip -->
    <Teleport to="body">
      <div 
        v-if="tooltip.visible"
        class="heatmap-tooltip"
        :style="{
          left: `${tooltip.x}px`,
          top: `${tooltip.y}px`,
          transform: 'translateX(-50%) translateY(-100%)'
        }"
      >
        <div class="tooltip-content">
          <div class="tooltip-date">{{ formatDate(tooltip.date) }}</div>
          <div class="tooltip-contributions">
            <span v-if="tooltip.count === 0">No contributions</span>
            <span v-else-if="tooltip.count === 1">1 contribution</span>
            <span v-else>{{ tooltip.count }} contributions</span>
          </div>
          <div class="tooltip-level">{{ getLevelDescription(tooltip.level) }}</div>
        </div>
        <div class="tooltip-arrow"></div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.activity-heatmap-container {
  @apply w-full max-w-6xl mx-auto px-4;
  border-radius: 6px;
}

.contribution-header {
  @apply flex justify-between items-center py-4;
}

.contribution-controls {
  @apply flex items-center gap-2;
}

.settings-button {
  @apply bg-gray-100 dark:bg-[#21262d] rounded-md px-3 py-1 text-sm border border-gray-300 dark:border-[#30363d] hover:border-gray-400 dark:hover:border-[#8b949e];
}

.year-button {
  @apply bg-blue-600 dark:bg-[#1064eb] border-blue-600 dark:border-[#1064eb] hover:bg-blue-700 dark:hover:bg-[#1a7bf5] hover:border-blue-700 dark:hover:border-[#1a7bf5] text-white;
}

.calendar-container {
  @apply border border-gray-300 dark:border-[#30363d] rounded-md overflow-hidden mb-8;
}

.months-header {
  @apply border-b border-gray-300 dark:border-[#30363d] px-2 py-1;
}

.month-names {
  @apply flex;
}

.empty-cell {
  @apply w-9;
}

.month-label {
  @apply text-xs text-center flex-1 text-gray-600 dark:text-gray-400;
}

.calendar-body {
  @apply flex p-2;
}

.day-labels {
  @apply flex flex-col justify-between pr-2 py-1;
}

.day-label {
  @apply text-xs h-4 text-right text-gray-600 dark:text-gray-400;
}

.calendar-grid {
  @apply flex-1;
}

.calendar-row {
  @apply flex h-4 mb-[3px];
}

.calendar-cell {
  @apply w-[11px] h-[11px] mr-[3px] rounded-sm;
}

/* Light mode activity levels */
.light .activity-level-0, :root:not(.dark) .activity-level-0 {
  @apply bg-gray-100;
}

.light .activity-level-1, :root:not(.dark) .activity-level-1 {
  @apply bg-green-100;
}

.light .activity-level-2, :root:not(.dark) .activity-level-2 {
  @apply bg-green-300;
}

.light .activity-level-3, :root:not(.dark) .activity-level-3 {
  @apply bg-green-500;
}

.light .activity-level-4, :root:not(.dark) .activity-level-4 {
  @apply bg-green-700;
}

/* Dark mode activity levels */
.dark .activity-level-0 {
  @apply bg-[#161b22];
}

.dark .activity-level-1 {
  @apply bg-[#0e4429];
}

.dark .activity-level-2 {
  @apply bg-[#006d32];
}

.dark .activity-level-3 {
  @apply bg-[#26a641];
}

.dark .activity-level-4 {
  @apply bg-[#39d353];
}

.calendar-footer {
  @apply flex justify-end px-4 py-2 border-t border-gray-300 dark:border-[#30363d];
}

.activity-info {
  @apply flex items-center text-xs gap-2 text-gray-600 dark:text-gray-400;
}

.activity-level-boxes {
  @apply flex gap-1;
}

.activity-level-box {
  @apply w-[10px] h-[10px] rounded-sm;
}

/* Loading States */
.loading-state {
  @apply opacity-100;
  animation: fadeIn 0.3s ease-in-out;
}

@keyframes fadeIn {
  from { opacity: 0; }
  to { opacity: 1; }
}

@keyframes shimmer {
  0% { background-position: -200px 0; }
  100% { background-position: calc(200px + 100%) 0; }
}

.skeleton-text {
  @apply bg-gray-200 dark:bg-gray-700 rounded relative overflow-hidden;
  background: linear-gradient(90deg, 
    theme('colors.gray.200') 25%, 
    theme('colors.gray.100') 50%, 
    theme('colors.gray.200') 75%
  );
  background-size: 200px 100%;
  animation: shimmer 1.5s infinite;
}

.dark .skeleton-text {
  background: linear-gradient(90deg, 
    theme('colors.gray.700') 25%, 
    theme('colors.gray.600') 50%, 
    theme('colors.gray.700') 75%
  );
  background-size: 200px 100%;
}

.skeleton-stats {
  @apply flex flex-col;
}

.skeleton-spinner {
  @apply w-6 h-6 border-2 border-gray-300 border-t-gray-600 rounded-full animate-spin;
}

.skeleton-calendar {
  @apply border border-gray-300 dark:border-[#30363d] rounded-md overflow-hidden bg-white dark:bg-gray-900;
}

.skeleton-months {
  @apply flex border-b border-gray-300 dark:border-[#30363d] px-2 py-1;
}

.skeleton-month-spacer {
  @apply w-9;
}

.skeleton-month {
  @apply bg-gray-200 dark:bg-gray-700 h-4 flex-1 rounded mx-1 relative overflow-hidden;
  background: linear-gradient(90deg, 
    theme('colors.gray.200') 25%, 
    theme('colors.gray.100') 50%, 
    theme('colors.gray.200') 75%
  );
  background-size: 200px 100%;
  animation: shimmer 1.5s infinite;
}

.dark .skeleton-month {
  background: linear-gradient(90deg, 
    theme('colors.gray.700') 25%, 
    theme('colors.gray.600') 50%, 
    theme('colors.gray.700') 75%
  );
  background-size: 200px 100%;
}

.skeleton-body {
  @apply flex p-2;
}

.skeleton-day-labels {
  @apply flex flex-col justify-between pr-2 py-1;
}

.skeleton-day-label {
  @apply bg-gray-200 dark:bg-gray-700 h-3 w-6 rounded mb-4 relative overflow-hidden;
  background: linear-gradient(90deg, 
    theme('colors.gray.200') 25%, 
    theme('colors.gray.100') 50%, 
    theme('colors.gray.200') 75%
  );
  background-size: 200px 100%;
  animation: shimmer 1.5s infinite;
}

.dark .skeleton-day-label {
  background: linear-gradient(90deg, 
    theme('colors.gray.700') 25%, 
    theme('colors.gray.600') 50%, 
    theme('colors.gray.700') 75%
  );
  background-size: 200px 100%;
}

.skeleton-grid {
  @apply flex-1;
}

.skeleton-row {
  @apply flex h-4 mb-[3px];
}

.skeleton-cell {
  @apply w-[11px] h-[11px] bg-gray-200 dark:bg-gray-700 rounded-sm mr-[3px] relative overflow-hidden;
  background: linear-gradient(90deg, 
    theme('colors.gray.200') 25%, 
    theme('colors.gray.100') 50%, 
    theme('colors.gray.200') 75%
  );
  background-size: 200px 100%;
  animation: shimmer 1.5s infinite;
}

.dark .skeleton-cell {
  background: linear-gradient(90deg, 
    theme('colors.gray.700') 25%, 
    theme('colors.gray.600') 50%, 
    theme('colors.gray.700') 75%
  );
  background-size: 200px 100%;
}

.skeleton-footer {
  @apply flex justify-end px-4 py-2 border-t border-gray-300 dark:border-[#30363d];
}

.skeleton-activity-info {
  @apply flex items-center gap-2;
}

.skeleton-level-boxes {
  @apply flex gap-1;
}

.skeleton-level-box {
  @apply w-[10px] h-[10px] bg-gray-200 dark:bg-gray-700 rounded-sm relative overflow-hidden;
  background: linear-gradient(90deg, 
    theme('colors.gray.200') 25%, 
    theme('colors.gray.100') 50%, 
    theme('colors.gray.200') 75%
  );
  background-size: 200px 100%;
  animation: shimmer 1.5s infinite;
}

.dark .skeleton-level-box {
  background: linear-gradient(90deg, 
    theme('colors.gray.700') 25%, 
    theme('colors.gray.600') 50%, 
    theme('colors.gray.700') 75%
  );
  background-size: 200px 100%;
}

/* Error States */
.error-state {
  @apply flex items-center justify-center min-h-[200px] border border-red-200 dark:border-red-800 rounded-md bg-red-50 dark:bg-red-900/20;
}

.error-content {
  @apply text-center p-6;
}

.error-icon {
  @apply text-4xl mb-4;
}

.error-title {
  @apply text-lg font-semibold text-red-800 dark:text-red-200 mb-2;
}

.error-message {
  @apply text-red-600 dark:text-red-300 mb-4;
}

.retry-button {
  @apply bg-red-600 hover:bg-red-700 disabled:bg-red-400 text-white px-4 py-2 rounded-md transition-colors;
}

/* Enhanced Interactivity */
.calendar-cell.clickable {
  @apply cursor-pointer transition-all duration-200;
}

.calendar-cell.clickable:hover {
  @apply ring-2 ring-blue-500 ring-opacity-50 scale-110 z-10 relative;
}

.calendar-cell.has-activity:hover {
  @apply shadow-lg;
}

.calendar-cell.clickable:focus {
  @apply ring-2 ring-blue-500 ring-opacity-75 scale-110 z-10 relative outline-none;
}

/* Activity level hover effects */
.activity-level-1.clickable:hover {
  @apply ring-green-400;
}

.activity-level-2.clickable:hover {
  @apply ring-green-500;
}

.activity-level-3.clickable:hover {
  @apply ring-green-600;
}

.activity-level-4.clickable:hover {
  @apply ring-green-700;
}

/* Loading Indicator */
.loading-indicator {
  @apply flex items-center;
}

.spinner {
  @apply w-4 h-4 border-2 border-gray-300 border-t-blue-600 rounded-full animate-spin;
}

/* Contribution Stats */
.contribution-header {
  @apply flex justify-between items-start py-4;
}

.contribution-stats {
  @apply flex flex-col gap-1;
}

.streak-info {
  @apply text-sm text-gray-600 dark:text-gray-400;
}

.longest-streak {
  @apply text-gray-500 dark:text-gray-500;
}

/* Responsive Design */
@media (max-width: 768px) {
  .activity-heatmap-container {
    @apply px-2;
  }
  
  .calendar-container {
    @apply overflow-x-auto;
  }
  
  .calendar-grid {
    @apply min-w-[700px];
  }
  
  .contribution-header {
    @apply flex-col items-start gap-2;
  }
}

/* Accessibility Improvements */
.calendar-cell:focus {
  @apply outline-none ring-2 ring-blue-500 ring-opacity-50;
}

.calendar-cell[tabindex="0"] {
  @apply focus:ring-2 focus:ring-blue-500 focus:ring-opacity-50;
}

/* Smooth Transitions */
.heatmap-content {
  @apply transition-all duration-500 ease-in-out;
  animation: slideInUp 0.4s ease-out;
}

@keyframes slideInUp {
  from {
    opacity: 0;
    transform: translateY(20px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.calendar-cell {
  @apply transition-all duration-200 ease-in-out;
}

/* Data update transitions */
.calendar-grid {
  @apply transition-opacity duration-300;
}

.calendar-container {
  @apply transition-all duration-300 ease-in-out;
}

/* Loading to content transition */
.loading-state {
  @apply transition-opacity duration-300;
}

.loading-state.fade-out {
  @apply opacity-0;
}

/* Error state transitions */
.error-state {
  @apply transition-all duration-300 ease-in-out;
  animation: slideInUp 0.4s ease-out;
}

/* Contribution header transitions */
.contribution-header {
  @apply transition-all duration-300;
}

.contribution-stats {
  @apply transition-opacity duration-300;
}

/* Progressive loading animation */
@keyframes progressiveLoad {
  0% { opacity: 0; transform: scale(0.8); }
  100% { opacity: 1; transform: scale(1); }
}

.calendar-cell.loaded {
  animation: progressiveLoad 0.3s ease-out forwards;
}

/* Staggered animation for calendar cells */
.calendar-row:nth-child(1) .calendar-cell { animation-delay: 0.05s; }
.calendar-row:nth-child(2) .calendar-cell { animation-delay: 0.1s; }
.calendar-row:nth-child(3) .calendar-cell { animation-delay: 0.15s; }
.calendar-row:nth-child(4) .calendar-cell { animation-delay: 0.2s; }
.calendar-row:nth-child(5) .calendar-cell { animation-delay: 0.25s; }
.calendar-row:nth-child(6) .calendar-cell { animation-delay: 0.3s; }
.calendar-row:nth-child(7) .calendar-cell { animation-delay: 0.35s; }

/* Custom Tooltip Styles */
.heatmap-tooltip {
  @apply fixed z-50 pointer-events-none;
}

.tooltip-content {
  @apply bg-gray-900 dark:bg-gray-800 text-white text-sm rounded-lg px-3 py-2 shadow-lg border border-gray-700;
  min-width: 180px;
}

.tooltip-date {
  @apply font-semibold text-gray-100 mb-1;
}

.tooltip-contributions {
  @apply text-gray-200 mb-1;
}

.tooltip-level {
  @apply text-xs text-gray-300 italic;
}

.tooltip-arrow {
  @apply absolute top-full left-1/2 transform -translate-x-1/2;
  width: 0;
  height: 0;
  border-left: 6px solid transparent;
  border-right: 6px solid transparent;
  border-top: 6px solid theme('colors.gray.900');
}

.dark .tooltip-arrow {
  border-top-color: theme('colors.gray.800');
}

/* Contribution Level Color Coding */
.activity-level-0 {
  @apply bg-gray-100 dark:bg-[#161b22];
}

.activity-level-1 {
  @apply bg-green-100 dark:bg-[#0e4429];
}

.activity-level-2 {
  @apply bg-green-300 dark:bg-[#006d32];
}

.activity-level-3 {
  @apply bg-green-500 dark:bg-[#26a641];
}

.activity-level-4 {
  @apply bg-green-700 dark:bg-[#39d353];
}

/* Streak Information Styling */
.streak-info {
  @apply flex items-center gap-2;
}

.streak-info::before {
  content: "🔥";
  @apply text-orange-500;
}

/* Animation for new contributions */
@keyframes contribution-pulse {
  0%, 100% { transform: scale(1); }
  50% { transform: scale(1.1); }
}

.calendar-cell.new-contribution {
  animation: contribution-pulse 0.6s ease-in-out;
}

/* High contrast mode support */
@media (prefers-contrast: high) {
  .activity-level-0 {
    @apply border border-gray-400;
  }
  
  .activity-level-1 {
    @apply bg-green-200 border border-green-400;
  }
  
  .activity-level-2 {
    @apply bg-green-400 border border-green-600;
  }
  
  .activity-level-3 {
    @apply bg-green-600 border border-green-800;
  }
  
  .activity-level-4 {
    @apply bg-green-800 border border-green-900;
  }
}

/* Reduced motion support */
@media (prefers-reduced-motion: reduce) {
  .calendar-cell {
    @apply transition-none;
  }
  
  .calendar-cell.clickable:hover {
    @apply scale-100;
  }
  
  .new-contribution {
    animation: none;
  }
}
</style> 