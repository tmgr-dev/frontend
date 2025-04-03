<script setup lang="ts">
import { ref, onMounted, computed } from 'vue';

// Make the heatmap smaller for better visibility
const days = 52; // Just use 52 cells (one per week) for a more compact display
const activityData = ref<number[]>([]);
const activityLabels = ref<string[]>([]);
const mostActive = ref(0);

// Activity level descriptions
const activityLevels = [
  'No activity',
  'Low activity',
  'Medium activity',
  'High activity',
  'Very high activity'
];

// Generate dates
const generateDates = () => {
  const dates = [];
  const today = new Date();
  
  for (let i = days - 1; i >= 0; i--) {
    const date = new Date(today);
    date.setDate(today.getDate() - (i * 7)); // One entry per week
    dates.push(date.toISOString().split('T')[0]);
  }
  
  return dates;
};

// Generate simple random activity
const generateActivityData = () => {
  const data = [];
  
  for (let i = 0; i < days; i++) {
    const activity = Math.floor(Math.random() * 20);
    data.push(activity);
  }
  
  return { data, highest: 20 };
};

// Function to determine activity level (0-4) for CSS class
const getActivityLevel = (count: number) => {
  if (count === 0) return 0;
  if (count < 5) return 1;
  if (count < 10) return 2;
  if (count < 15) return 3;
  return 4;
};

// Random factoids about productivity
const activityFactoids = [
  "Your most productive day was on a Tuesday",
  "You're 35% more productive in the morning",
  "You completed 127 tasks this year",
  "You've spent 342 hours on projects",
  "Your longest streak was 12 days"
];

// Select random factoids
const selectedFactoids = computed(() => {
  return activityFactoids.slice(0, 3);
});

onMounted(() => {
  activityLabels.value = generateDates();
  const { data, highest } = generateActivityData();
  activityData.value = data;
  mostActive.value = highest;
});
</script>

<template>
  <div class="activity-heatmap-container">
    <h2 class="text-xl font-semibold mb-4">Your activity</h2>
    
    <!-- Simple Activity Grid -->
    <div class="heatmap-visualization">
      <div class="simple-heatmap-grid">
        <div 
          v-for="(activity, index) in activityData" 
          :key="index"
          class="heatmap-cell"
          :class="`activity-level-${getActivityLevel(activity)}`"
          :title="`Week ${index + 1}: ${activity} contributions`"
        ></div>
      </div>
      
      <div class="heatmap-legend mt-4">
        <span class="legend-label">Less</span>
        <div class="legend-cells">
          <div 
            v-for="level in 5" 
            :key="level-1"
            class="legend-cell" 
            :class="`activity-level-${level-1}`"
            :title="activityLevels[level-1]"
          ></div>
        </div>
        <span class="legend-label">More</span>
      </div>
    </div>
    
    <div class="activity-stats mt-6">
      <div v-for="(factoid, i) in selectedFactoids" :key="i" class="stat-card">
        <div class="stat-content">{{ factoid }}</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.activity-heatmap-container {
  @apply w-full max-w-5xl mx-auto my-8 px-4 py-6 block;
  min-height: 20rem;
  border: 2px solid #e5e7eb;
}

.heatmap-visualization {
  @apply bg-white dark:bg-gray-900 rounded-lg p-6 shadow;
}

.simple-heatmap-grid {
  @apply flex flex-wrap justify-center gap-1;
  max-width: 800px;
  margin: 0 auto;
}

.heatmap-cell {
  width: 24px;
  height: 24px;
  border-radius: 4px;
  transition: transform 0.1s ease;
}

.heatmap-cell:hover {
  transform: scale(1.2);
}

.activity-level-0 {
  @apply bg-gray-100 dark:bg-gray-800;
}

.activity-level-1 {
  @apply bg-green-100 dark:bg-green-900;
}

.activity-level-2 {
  @apply bg-green-300 dark:bg-green-700;
}

.activity-level-3 {
  @apply bg-green-500 dark:bg-green-500;
}

.activity-level-4 {
  @apply bg-green-700 dark:bg-green-300;
}

.heatmap-legend {
  @apply flex items-center justify-center gap-2 text-xs text-gray-500;
}

.legend-cells {
  @apply flex gap-1;
}

.legend-cell {
  width: 16px;
  height: 16px;
  border-radius: 3px;
}

.activity-stats {
  @apply grid grid-cols-1 md:grid-cols-3 gap-4;
}

.stat-card {
  @apply bg-white dark:bg-gray-900 rounded-lg p-4 shadow;
}

.stat-content {
  @apply text-sm text-gray-700 dark:text-gray-300;
}
</style> 