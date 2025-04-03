<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useRoute } from 'vue-router';
import store from '@/store';
import HeatmapComponent from '@/components/HeatmapComponent.vue';
import TasksListComponent from '@/components/tasks/TasksListComponent.vue';
import { getTasks, Task, PaginationMeta } from '@/actions/tmgr/tasks';
import BaseLayout from '@/components/layouts/BaseLayout.vue';

const route = useRoute();
const isLoading = ref(true);
const tasks = ref<Task[]>([]);
const isLoadingActions = ref<Record<string, boolean>>({});
const pagination = ref<PaginationMeta>({
  current_page: 1,
  per_page: 5,
  total: 0,
  last_page: 1,
  from: 0,
  to: 0
});

onMounted(async () => {
  try {
    await loadRecentTasks();
    
    // Set page title with workspace name if available
    if (store.state.workspaces?.length) {
      // Get current workspace ID
      const currentWorkspaceId = store.state.user?.settings?.find(
        (setting: { key: string, value: any }) => setting.key === 'current_workspace'
      )?.value;
      
      // Find the workspace by ID
      const currentWorkspace = store.state.workspaces.find(
        (workspace: { id: number, name: string, code: string }) => 
          Number(workspace.id) === Number(currentWorkspaceId)
      );
      
      if (currentWorkspace?.name) {
        document.title = `${currentWorkspace.name} Dashboard`;
        store.commit('setMetaTitle', `${currentWorkspace.name} Dashboard`);
      } else {
        document.title = 'Dashboard';
        store.commit('setMetaTitle', 'Dashboard');
      }
    }
  } catch (e) {
    console.error(e);
  }
});

async function loadRecentTasks() {
  try {
    isLoading.value = true;
    
    const params = {
      params: {},
      page: 1,
      per_page: 5
    };
    
    const response = await getTasks(params);
    tasks.value = response.data;
    pagination.value = response.meta;
    
    // Setup loading actions state for tasks
    tasks.value.forEach((task) => {
      if (task.id) {
        isLoadingActions.value[`hide-${task.id}`] = false;
        isLoadingActions.value[`done-${task.id}`] = false;
        isLoadingActions.value[`start-${task.id}`] = false;
        isLoadingActions.value[`stop-${task.id}`] = false;
        isLoadingActions.value[`activate-${task.id}`] = false;
        isLoadingActions.value[`delete-${task.id}`] = false;
      }
    });
  } catch (e) {
    console.error(e);
  } finally {
    isLoading.value = false;
  }
}

function handlePageChange(page: number) {
  pagination.value.current_page = page;
  loadRecentTasks();
}

function handlePerPageChange(perPage: number) {
  pagination.value.per_page = perPage;
  pagination.value.current_page = 1; // Reset to first page when changing items per page
  loadRecentTasks();
}
</script>

<template>
  <teleport to="title">
    Dashboard
  </teleport>

  <BaseLayout>
    <template #body>
      <div class="dashboard-container">
        <div class="dashboard-header">
          <h1 class="text-3xl font-bold">Dashboard</h1>
        </div>
        
        <!-- Activity Heatmap Section -->
        <section class="dashboard-section heatmap-section">
          <HeatmapComponent />
        </section>
        
        <!-- Recent Tasks Section -->
        <section class="dashboard-section mt-8">
          <div class="section-header mb-4">
            <h2 class="text-xl font-semibold">Recent Tasks</h2>
            <router-link 
              to="/list" 
              class="text-sm text-blue-500 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300"
            >
              View all tasks →
            </router-link>
          </div>
          
          <div class="recent-tasks-container">
            <div v-if="isLoading" class="loading-placeholder">
              <div class="animate-pulse flex flex-col gap-4">
                <div v-for="i in 5" :key="i" class="h-16 bg-gray-200 dark:bg-gray-800 rounded"></div>
              </div>
            </div>
            
            <TasksListComponent 
              v-else 
              :tasks="tasks" 
              :pagination="pagination"
              :is-loading-actions="isLoadingActions"
              :loading-action-tasks-ids="[]"
              :show-category-badges="true"
              :use-task-status-for-buttons="false"
              :draggable="false"
              :has-selectable="false"
              @reload-tasks="loadRecentTasks"
              @page-change="handlePageChange"
              @per-page-change="handlePerPageChange"
              class="recent-tasks-list"
            />
          </div>
        </section>
      </div>
    </template>
  </BaseLayout>
</template>

<style scoped>
.dashboard-container {
  @apply w-full max-w-6xl mx-auto px-4 py-6;
  min-height: 100vh;
}

.dashboard-header {
  @apply mb-8;
}

.dashboard-section {
  @apply mb-10;
}

.heatmap-section {
  @apply rounded-lg;
  color: #fff;
  background-color: transparent;
  border: none;
  min-height: auto;
  padding: 0;
}

.section-header {
  @apply flex justify-between items-center;
}

.recent-tasks-container {
  @apply bg-white dark:bg-gray-900 rounded-lg p-4 shadow;
}

.loading-placeholder {
  @apply w-full;
}

.recent-tasks-list {
  @apply w-full;
}
</style> 