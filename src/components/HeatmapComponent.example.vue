<template>
  <div class="heatmap-example">
    <h2>Enhanced Heatmap Component Example</h2>
    
    <!-- Basic Usage -->
    <div class="example-section">
      <h3>Basic Usage</h3>
      <HeatmapComponent 
        :workspace-id="currentWorkspaceId"
        :year="2024"
        @date-click="handleDateClick"
        @retry="handleRetry"
      />
    </div>
    
    <!-- With Custom User -->
    <div class="example-section">
      <h3>Specific User Heatmap</h3>
      <HeatmapComponent 
        :workspace-id="currentWorkspaceId"
        :user-id="selectedUserId"
        :year="2024"
        @date-click="handleDateClick"
        @retry="handleRetry"
      />
    </div>
    
    <!-- With External Loading State -->
    <div class="example-section">
      <h3>With External Loading Control</h3>
      <button @click="refreshHeatmap" :disabled="isRefreshing">
        {{ isRefreshing ? 'Refreshing...' : 'Refresh Heatmap' }}
      </button>
      <HeatmapComponent 
        :workspace-id="currentWorkspaceId"
        :year="selectedYear"
        :loading="isRefreshing"
        :error="heatmapError"
        @update:loading="isRefreshing = $event"
        @update:error="heatmapError = $event"
        @date-click="handleDateClick"
        @retry="handleRetry"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import HeatmapComponent from './HeatmapComponent.vue';
import type { ActionError } from '@/types/dashboard';

// Example state
const currentWorkspaceId = ref(1);
const selectedUserId = ref(123);
const selectedYear = ref(2024);
const isRefreshing = ref(false);
const heatmapError = ref<ActionError | null>(null);

// Example event handlers
const handleDateClick = (date: string, count: number) => {
  console.log(`Date clicked: ${date} with ${count} contributions`);
  // Navigate to filtered view or show detailed activities for this date
};

const handleRetry = () => {
  console.log('Retry requested');
  heatmapError.value = null;
};

const refreshHeatmap = async () => {
  isRefreshing.value = true;
  // Simulate API call
  await new Promise(resolve => setTimeout(resolve, 2000));
  isRefreshing.value = false;
};
</script>

<style scoped>
.heatmap-example {
  @apply p-6 space-y-8;
}

.example-section {
  @apply space-y-4;
}

.example-section h3 {
  @apply text-lg font-semibold text-gray-800 dark:text-gray-200;
}

button {
  @apply bg-blue-600 hover:bg-blue-700 disabled:bg-blue-400 text-white px-4 py-2 rounded-md transition-colors;
}
</style>