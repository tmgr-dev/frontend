<script setup lang="ts">
import { ref, onMounted, computed } from 'vue';

// Full year of data (365 days)
const days = 365;
const activityData = ref<number[]>([]);
const activityLabels = ref<string[]>([]);
const mostActive = ref(20);

// Random total contributions
const totalContributions = ref(Math.floor(Math.random() * 300) + 100);

// Days of week for y-axis
const daysOfWeek = ['Mon', 'Wed', 'Fri'];

// Months for x-axis
const months = ['Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec', 'Jan', 'Feb', 'Mar'];

// Activity level descriptions
const activityLevels = [
  'No activity',
  'Low activity',
  'Medium activity',
  'High activity',
  'Very high activity'
];

// Generate random activity data for a year
const generateActivityData = () => {
  const data = [];
  
  for (let i = 0; i < days; i++) {
    // Higher probability of no activity (0)
    const random = Math.random();
    let activity = 0;
    
    if (random > 0.75) {
      activity = Math.floor(Math.random() * 5) + 1; // 1-5
    } else if (random > 0.9) {
      activity = Math.floor(Math.random() * 5) + 5; // 5-10
    } else if (random > 0.97) {
      activity = Math.floor(Math.random() * 10) + 10; // 10-20
    }
    
    data.push(activity);
  }
  
  return data;
};

// Generate dates for the past year
const generateDates = () => {
  const dates = [];
  const today = new Date();
  
  for (let i = days - 1; i >= 0; i--) {
    const date = new Date(today);
    date.setDate(today.getDate() - i);
    dates.push(date.toISOString().split('T')[0]);
  }
  
  return dates;
};

// Function to determine activity level (0-4) for CSS class
const getActivityLevel = (count: number) => {
  if (count === 0) return 0;
  if (count < 4) return 1;
  if (count < 8) return 2;
  if (count < 12) return 3;
  return 4;
};

// Format date for tooltip
const formatDate = (dateStr: string) => {
  const date = new Date(dateStr);
  return date.toLocaleDateString('en-US', { 
    weekday: 'long',
    year: 'numeric', 
    month: 'long', 
    day: 'numeric'
  });
};

// Define the type for a cell
interface GridCell {
  value: number | null;
  date: string | null;
  index: number | null;
}

// Generate 7x53 grid (53 weeks x 7 days)
const calendarGrid = computed(() => {
  const grid: GridCell[][] = [];
  
  // Initialize with empty grid
  for (let row = 0; row < 7; row++) {
    const rowData: GridCell[] = [];
    for (let col = 0; col < 53; col++) {
      rowData.push({ value: null, date: null, index: null });
    }
    grid.push(rowData);
  }
  
  // Fill grid with data
  if (activityData.value.length > 0) {
    let currentDay = new Date(activityLabels.value[0]);
    let dayOfWeek = currentDay.getDay(); // 0 = Sunday, 1 = Monday, etc.
    dayOfWeek = dayOfWeek === 0 ? 6 : dayOfWeek - 1; // Adjust to 0 = Monday, 6 = Sunday
    
    let week = 0;
    
    for (let i = 0; i < activityData.value.length; i++) {
      grid[dayOfWeek][week] = {
        value: activityData.value[i],
        date: activityLabels.value[i],
        index: i
      };
      
      dayOfWeek++;
      if (dayOfWeek >= 7) {
        dayOfWeek = 0;
        week++;
      }
    }
  }
  
  return grid;
});

onMounted(() => {
  activityLabels.value = generateDates();
  activityData.value = generateActivityData();
});
</script>

<template>
  <div class="activity-heatmap-container">
    <div class="contribution-header">
      <div class="font-medium text-xl">{{ totalContributions }} contributions in the last year</div>
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
                cell.value !== null ? `activity-level-${getActivityLevel(cell.value)}` : 'empty-cell'
              ]"
              :title="cell.date ? `${formatDate(cell.date)}: ${cell.value} contributions` : ''"
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
</style> 