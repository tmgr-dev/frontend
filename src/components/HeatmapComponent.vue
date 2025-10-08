<template>
  <div>
    <!-- Header with year and controls -->
    <div class="heatmap-header">
      <div class="year-controls">
        <button 
          @click="previousYear" 
          :disabled="isLoading"
          class="year-btn"
        >
          ←
        </button>
        <h3 class="year-title">{{ props.year }}</h3>
        <button 
          @click="nextYear" 
          :disabled="isLoading || props.year >= new Date().getFullYear()"
          class="year-btn"
        >
          →
        </button>
      </div>
      
      <div class="heatmap-controls">
        <label class="control-item">
          <input 
            type="checkbox" 
            v-model="includeWeekends" 
            @change="onIncludeWeekendsChange"
            :disabled="isLoading"
          >
          Include weekends
        </label>
      </div>
    </div>

    <!-- Loading state -->
    <div v-if="isLoading" class="loading-container">
      <div class="loading-spinner"></div>
      <p>Loading heatmap data...</p>
    </div>

    <!-- Error state -->
    <div v-else-if="currentError" class="error-container">
      <div class="error-icon">⚠️</div>
      <p class="error-message">{{ currentError }}</p>
      <button @click="retry" class="retry-btn">Retry</button>
    </div>

    <!-- Heatmap visualization -->
    <div v-else-if="currentHeatmapData" class="heatmap-visualization">
      <!-- Vue3 Calendar Heatmap -->
      <div class="heatmap-wrapper">
        <calendar-heatmap
          v-if="heatmapValues.length > 0"
          :values="heatmapValues"
          :end-date="endDate"
          :range-color="rangeColor"
          :tooltip-unit="tooltipUnit"
          :round="round"
          :vertical="vertical"
          :no-data-text="noDataText"
          :max="max"
          @click="onDateClick"
        />
        
        <!-- No data fallback -->
        <div v-else class="no-data-fallback">
          <p>No contribution data available for {{ props.year }}</p>
        </div>
      </div>

      <!-- Stats -->
      <div class="heatmap-stats">
        <div class="stat-item">
          <span class="stat-label">Total contributions:</span>
          <span class="stat-value">{{ totalContributions }}</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">Current streak:</span>
          <span class="stat-value">{{ streakInfo.current }} days</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">Longest streak:</span>
          <span class="stat-value">{{ streakInfo.longest }} days</span>
        </div>
      </div>
    </div>

    <!-- Empty state -->
    <div v-else class="empty-state">
      <div class="empty-icon">📅</div>
      <p>No contribution data available</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed, watch } from 'vue';
import { getUserHeatmap } from '@/actions/tmgr/dashboard';
import type { HeatmapData, ActionError } from '@/types/dashboard';
// Import vue3-calendar-heatmap according to documentation
import { CalendarHeatmap } from '@silverwind/vue3-calendar-heatmap';

// Props
interface Props {
  workspaceId: number;
  userId?: number;
  year?: number;
  loading?: boolean;
  error?: ActionError | null;
  data?: HeatmapData | null;
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
  'update:year': [year: number];
  'date-click': [date: string, count: number];
  'retry': [];
}>();

// State
const heatmapData = ref<HeatmapData | null>(null);
const internalLoading = ref(false);
const internalError = ref<ActionError | null>(null);
const includeWeekends = ref(true);

// Use provided data or internal data
const currentHeatmapData = computed(() => props.data || heatmapData.value);

// Computed loading and error states
const isLoading = computed(() => props.loading || internalLoading.value);
const currentError = computed(() => props.error || internalError.value);

// Computed stats
const totalContributions = computed(() => {
  if (!currentHeatmapData.value) return 0;
  return currentHeatmapData.value.contributions.reduce((sum, contrib) => sum + contrib.count, 0);
});

const streakInfo = computed(() => {
  if (!currentHeatmapData.value) return { current: 0, longest: 0 };
  const streak = (currentHeatmapData.value as any).streak;
  return {
    current: streak?.current || 0,
    longest: streak?.longest || 0
  };
});

// Transform API data for vue3-calendar-heatmap
const heatmapValues = computed(() => {
  if (!currentHeatmapData.value?.contributions) {
    return [];
  }
  
  return currentHeatmapData.value.contributions.map(contrib => ({
    date: contrib.date,
    count: contrib.count
  }));
});

// Calendar heatmap configuration
const endDate = computed(() => {
  const year = props.year;
  const currentYear = new Date().getFullYear();
  
  if (year === currentYear) {
    return new Date(); // Current date for current year
  } else {
    return new Date(year, 11, 31); // End of the specified year
  }
});

const rangeColor = ['#ebedf0', '#c6e48b', '#7bc96f', '#239a3b', '#196127'];
const tooltipUnit = 'contributions';
const round = 2;
const vertical = false;
const noDataText = 'No contributions';
const max = computed(() => {
  if (!currentHeatmapData.value?.contributions) return 0;
  return Math.max(...currentHeatmapData.value.contributions.map(c => c.count));
});

// Load heatmap data from API
const loadHeatmapData = async () => {
  if (isLoading.value) return;
  
  // If data is provided via props, don't load from API
  if (props.data) return;
  
  try {
    internalLoading.value = true;
    internalError.value = null;
    emit('update:loading', true);
    emit('update:error', null);
    
    const result = await getUserHeatmap(props.workspaceId, {
      year: props.year,
      user_id: props.userId,
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      include_weekends: includeWeekends.value
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

// Year navigation
const previousYear = () => {
  emit('update:year', props.year - 1);
};

const nextYear = () => {
  emit('update:year', props.year + 1);
};

// Controls
const onIncludeWeekendsChange = () => {
  loadHeatmapData();
};

// Retry function
const retry = () => {
  emit('retry');
  loadHeatmapData();
};

// Handle date click
const onDateClick = (event: any) => {
  const date = event.date;
  const count = event.count || 0;
  emit('date-click', date, count);
};


// Watch for year changes
watch(() => props.year, () => {
  loadHeatmapData();
});

// Lifecycle
onMounted(() => {
  loadHeatmapData();
});
</script>

<style scoped>
.heatmap-container {
  background: white;
  border-radius: 8px;
  padding: 20px;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
}

.heatmap-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.year-controls {
  display: flex;
  align-items: center;
  gap: 10px;
}

.year-btn {
  background: #f5f5f5;
  border: 1px solid #ddd;
  border-radius: 4px;
  padding: 8px 12px;
  cursor: pointer;
  font-size: 16px;
  transition: all 0.2s;
}

.year-btn:hover:not(:disabled) {
  background: #e9ecef;
}

.year-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.year-title {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  color: #333;
}

.heatmap-controls {
  display: flex;
  align-items: center;
  gap: 15px;
}

.control-item {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 14px;
  color: #666;
  cursor: pointer;
}

.control-item input[type="checkbox"] {
  margin: 0;
}

.loading-container {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 40px;
  color: #666;
}

.loading-spinner {
  width: 32px;
  height: 32px;
  border: 3px solid #f3f3f3;
  border-top: 3px solid #007bff;
  border-radius: 50%;
  animation: spin 1s linear infinite;
  margin-bottom: 10px;
}

@keyframes spin {
  0% { transform: rotate(0deg); }
  100% { transform: rotate(360deg); }
}

.error-container {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 40px;
  color: #dc3545;
}

.error-icon {
  font-size: 32px;
  margin-bottom: 10px;
}

.error-message {
  margin: 0 0 15px 0;
  text-align: center;
}

.retry-btn {
  background: #007bff;
  color: white;
  border: none;
  border-radius: 4px;
  padding: 8px 16px;
  cursor: pointer;
  font-size: 14px;
  transition: background 0.2s;
}

.retry-btn:hover {
  background: #0056b3;
}

.heatmap-visualization {
  margin-bottom: 20px;
}

.heatmap-wrapper {
  width: 100%;
  overflow-x: auto;
  min-height: 200px;
  position: relative;
}

.custom-heatmap {
  width: 100%;
  overflow-x: auto;
}

.no-data-fallback {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 200px;
  background: #f8f9fa;
  border-radius: 6px;
  border: 2px dashed #dee2e6;
  color: #666;
}


.heatmap-stats {
  display: flex;
  justify-content: space-around;
  margin-top: 20px;
  padding: 15px;
  background: #f8f9fa;
  border-radius: 6px;
}

.stat-item {
  text-align: center;
}

.stat-label {
  display: block;
  font-size: 12px;
  color: #666;
  margin-bottom: 4px;
}

.stat-value {
  display: block;
  font-size: 18px;
  font-weight: 600;
  color: #333;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 40px;
  color: #666;
}

.empty-icon {
  font-size: 48px;
  margin-bottom: 10px;
}

/* Dark mode support */
@media (prefers-color-scheme: dark) {
  .heatmap-container {
    background: #1a1a1a;
    color: #e0e0e0;
  }
  
  .year-title {
    color: #e0e0e0;
  }
  
  .stat-value {
    color: #e0e0e0;
  }
  
  .heatmap-stats {
    background: #2d2d2d;
  }
  
  .stat-label {
    color: #999;
  }
}
</style>