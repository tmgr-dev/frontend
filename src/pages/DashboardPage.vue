<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useRoute } from 'vue-router';
import store from '@/store';
import HeatmapComponent from '@/components/HeatmapComponent.vue';
import { getTasks, Task, PaginationMeta } from '@/actions/tmgr/tasks';
import BaseLayout from '@/components/layouts/BaseLayout.vue';

const route = useRoute();
const isLoading = ref(true);
const activities = ref([
  {
    id: 1,
    type: 'task_created',
    title: 'New task created',
    description: 'Marketing campaign outline',
    user: 'Alex Smith',
    timestamp: '2 hours ago',
    icon: 'add_task'
  },
  {
    id: 2,
    type: 'category_updated',
    title: 'Category updated',
    description: 'Development sprints',
    user: 'Maria Johnson',
    timestamp: '4 hours ago',
    icon: 'drive_file_rename_outline'
  },
  {
    id: 3,
    type: 'task_completed',
    title: 'Task completed',
    description: 'Q3 sales report',
    user: 'John Doe',
    timestamp: 'Yesterday',
    icon: 'task_alt'
  },
  {
    id: 4,
    type: 'member_joined',
    title: 'New member joined',
    description: 'Sarah Williams joined the workspace',
    user: 'System',
    timestamp: '2 days ago',
    icon: 'person_add'
  },
  {
    id: 5,
    type: 'task_assigned',
    title: 'Task assigned',
    description: 'Website redesign',
    user: 'Alex Smith',
    timestamp: '3 days ago',
    icon: 'assignment_ind'
  }
]);

onMounted(async () => {
  try {
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
    
    // Simulate loading
    setTimeout(() => {
      isLoading.value = false;
    }, 1000);
  } catch (e) {
    console.error(e);
    isLoading.value = false;
  }
});
</script>

<template>
  <teleport to="title">
    Dashboard
  </teleport>

  <BaseLayout>
    <template #body>
      <div class="dashboard-container">
        <div class="dashboard-header flex items-center justify-between">
          <div class="flex items-center gap-3">
            <h1 class="text-3xl font-bold">Dashboard</h1>
            <span class="rounded-full bg-blue-100 px-2 py-1 text-xs font-semibold text-blue-800 dark:bg-blue-900/50 dark:text-blue-200">
              Preview
            </span>
          </div>
        </div>
        
        <!-- Development Notice -->
        <div class="mb-6 rounded-lg bg-yellow-100 p-4 dark:bg-yellow-900/30">
          <div class="flex items-center">
            <span class="material-icons mr-2 text-yellow-600 dark:text-yellow-400">warning</span>
            <div>
              <h3 class="font-bold text-yellow-800 dark:text-yellow-300">Development Preview</h3>
              <p class="text-yellow-700 dark:text-yellow-200">
                This dashboard is currently under development. Some features might not work as expected.
                We're working to improve your experience.
              </p>
            </div>
          </div>
        </div>
        
        <!-- Activity Heatmap Section -->
        <section class="dashboard-section heatmap-section">
          <HeatmapComponent />
        </section>
        
        <!-- Recent Workspace Activities Section -->
        <section class="dashboard-section mt-8">
          <div class="section-header mb-4">
            <h2 class="text-xl font-semibold">Recent Workspace Activities</h2>
            <a 
              href="#" 
              class="text-sm text-blue-500 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300"
            >
              View all activities →
            </a>
          </div>
          
          <div class="recent-activities-container">
            <div v-if="isLoading" class="loading-placeholder">
              <div class="animate-pulse flex flex-col gap-4">
                <div v-for="i in 5" :key="i" class="h-16 bg-gray-200 dark:bg-gray-800 rounded"></div>
              </div>
            </div>
            
            <div v-else class="space-y-3">
              <div 
                v-for="activity in activities" 
                :key="activity.id" 
                class="activity-item flex items-center gap-3 rounded-lg border border-gray-100 bg-white p-3 transition-colors hover:bg-gray-50 dark:border-gray-800 dark:bg-gray-900 dark:hover:bg-gray-800"
              >
                <div :class="`activity-icon ${activity.type} rounded-full p-2`">
                  <span class="material-icons text-white">{{ activity.icon }}</span>
                </div>
                
                <div class="flex-1">
                  <div class="flex items-center justify-between">
                    <h3 class="font-medium">{{ activity.title }}</h3>
                    <span class="text-xs text-gray-500 dark:text-gray-400">{{ activity.timestamp }}</span>
                  </div>
                  <p class="text-sm text-gray-600 dark:text-gray-300">{{ activity.description }}</p>
                  <div class="mt-1 text-xs text-gray-500 dark:text-gray-400">
                    by {{ activity.user }}
                  </div>
                </div>
              </div>
              
              <div class="flex justify-center pt-2">
                <button class="text-sm text-blue-500 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300">
                  Load more
                </button>
              </div>
            </div>
          </div>
        </section>
        
        <!-- Coming Soon Features -->
        <section class="dashboard-section">
          <h2 class="text-xl font-semibold mb-4">Upcoming Features</h2>
          <div class="grid gap-4 md:grid-cols-2">
            <!-- Analytics Feature -->
            <div class="rounded-lg border border-dashed border-indigo-300 bg-indigo-50 p-4 dark:border-indigo-800 dark:bg-indigo-900/20">
              <div class="flex items-center justify-between">
                <div>
                  <h3 class="text-lg font-semibold text-indigo-800 dark:text-indigo-400">Checkpoints</h3>
                  <p class="text-indigo-600 dark:text-indigo-300">
                    Task checkpoints!
                  </p>
                </div>
                <span class="rounded-md bg-indigo-100 px-3 py-1 text-sm font-bold text-indigo-800 dark:bg-indigo-800 dark:text-indigo-200">
                  Coming Soon
                </span>
              </div>
            </div>
            
            <!-- Team Collaboration Feature -->
            <div class="rounded-lg border border-dashed border-emerald-300 bg-emerald-50 p-4 dark:border-emerald-800 dark:bg-emerald-900/20">
              <div class="flex items-center justify-between">
                <div>
                  <h3 class="text-lg font-semibold text-emerald-800 dark:text-emerald-400">Files</h3>
                  <p class="text-emerald-600 dark:text-emerald-300">
                    Files attachments!
                  </p>
                </div>
                <span class="rounded-md bg-emerald-100 px-3 py-1 text-sm font-bold text-emerald-800 dark:bg-emerald-800 dark:text-emerald-200">
                  Coming Soon
                </span>
              </div>
            </div>
          </div>
        </section>
        
        <!-- Development Feedback Section -->
        <section class="dashboard-section">
          <div class="rounded-lg border border-gray-200 bg-white p-6 dark:border-gray-700 dark:bg-gray-900">
            <h3 class="mb-4 text-xl font-semibold">Help Us Improve</h3>
            <p class="mb-4 text-gray-600 dark:text-gray-300">
              We're actively developing this project and would appreciate your feedback. If you encounter any issues or have suggestions, please let us know.
            </p>
            <button 
              class="inline-flex items-center rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 dark:bg-blue-700 dark:hover:bg-blue-600"
            >
              <span class="material-icons mr-2">feedback</span>
              Submit Feedback
            </button>
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

.recent-activities-container {
  @apply bg-white dark:bg-gray-900 rounded-lg p-4 shadow;
}

.loading-placeholder {
  @apply w-full;
}

.activity-icon {
  @apply flex h-10 w-10 items-center justify-center;
}

.activity-icon.task_created {
  background-color: #4f46e5;
}

.activity-icon.category_updated {
  background-color: #0ea5e9;
}

.activity-icon.task_completed {
  background-color: #10b981;
}

.activity-icon.member_joined {
  background-color: #f59e0b;
}

.activity-icon.task_assigned {
  background-color: #8b5cf6;
}
</style> 