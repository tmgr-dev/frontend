<template>
  <div>
    <div v-if="loading" class="flex items-center justify-center py-8">
      <div class="h-8 w-8 animate-spin rounded-full border-4 border-blue-500 border-t-transparent"></div>
    </div>

    <div v-else-if="!hasActivity" class="rounded-lg border border-gray-300 bg-gray-50 p-8 text-center dark:border-gray-600 dark:bg-gray-800">
      <svg class="mx-auto mb-3 h-12 w-12 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"></path>
      </svg>
      <p class="text-gray-600 dark:text-gray-400">
        No git activity linked to this task yet
      </p>
      <p class="mt-2 text-sm text-gray-500 dark:text-gray-500">
        Use {{ categoryCode }}-{{ taskId }} in commits, branches, or PR titles to link them
      </p>
    </div>

    <div v-else class="space-y-6">
      <div v-if="activity.branches && activity.branches.length > 0">
        <h4 class="mb-3 flex items-center text-sm font-semibold text-gray-700 dark:text-gray-300">
          <svg class="mr-2 h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 7h12m0 0l-4-4m4 4l-4 4m0 6H4m0 0l4 4m-4-4l4-4"></path>
          </svg>
          Branches ({{ activity.branches.length }})
        </h4>
        <div class="space-y-2">
          <a
            v-for="branch in activity.branches"
            :key="branch.id"
            :href="branch.repository_url"
            target="_blank"
            class="flex items-center justify-between rounded-lg border border-gray-200 bg-white p-3 transition hover:border-blue-400 hover:bg-blue-50 dark:border-gray-700 dark:bg-gray-800 dark:hover:border-blue-600 dark:hover:bg-gray-700"
          >
            <div class="flex items-center">
              <span
                :class="[
                  'mr-3 rounded px-2 py-1 text-xs font-mono',
                  branch.is_default
                    ? 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400'
                    : 'bg-gray-100 text-gray-700 dark:bg-gray-700 dark:text-gray-300'
                ]"
              >
                {{ branch.name }}
              </span>
              <span v-if="branch.repository" class="text-xs text-gray-500 dark:text-gray-400">
                {{ branch.repository }}
              </span>
            </div>
            <svg class="h-4 w-4 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"></path>
            </svg>
          </a>
        </div>
      </div>

      <div v-if="activity.commits && activity.commits.length > 0">
        <h4 class="mb-3 flex items-center text-sm font-semibold text-gray-700 dark:text-gray-300">
          <svg class="mr-2 h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"></path>
          </svg>
          Commits ({{ activity.commits.length }})
        </h4>
        <div class="space-y-2">
          <a
            v-for="commit in activity.commits"
            :key="commit.id"
            :href="commit.url"
            target="_blank"
            class="block rounded-lg border border-gray-200 bg-white p-3 transition hover:border-blue-400 hover:bg-blue-50 dark:border-gray-700 dark:bg-gray-800 dark:hover:border-blue-600 dark:hover:bg-gray-700"
          >
            <div class="flex items-start justify-between">
              <div class="flex-1">
                <p class="text-sm text-gray-800 dark:text-gray-200">
                  {{ commit.message.split('\n')[0] }}
                </p>
                <div class="mt-1 flex items-center space-x-3 text-xs text-gray-500 dark:text-gray-400">
                  <span class="font-mono">{{ commit.short_sha }}</span>
                  <span>{{ commit.author_name }}</span>
                  <span>{{ formatDate(commit.committed_at) }}</span>
                </div>
              </div>
              <svg class="ml-2 h-4 w-4 flex-shrink-0 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"></path>
              </svg>
            </div>
          </a>
        </div>
      </div>

      <div v-if="activity.pull_requests && activity.pull_requests.length > 0">
        <h4 class="mb-3 flex items-center text-sm font-semibold text-gray-700 dark:text-gray-300">
          <svg class="mr-2 h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 7h12m0 0l-4-4m4 4l-4 4m0 6H4m0 0l4 4m-4-4l4-4"></path>
          </svg>
          Pull Requests ({{ activity.pull_requests.length }})
        </h4>
        <div class="space-y-2">
          <a
            v-for="pr in activity.pull_requests"
            :key="pr.id"
            :href="pr.url"
            target="_blank"
            class="block rounded-lg border border-gray-200 bg-white p-3 transition hover:border-blue-400 hover:bg-blue-50 dark:border-gray-700 dark:bg-gray-800 dark:hover:border-blue-600 dark:hover:bg-gray-700"
          >
            <div class="flex items-start justify-between">
              <div class="flex-1">
                <div class="flex items-center">
                  <span
                    :class="[
                      'mr-2 rounded px-2 py-1 text-xs font-semibold',
                      pr.state === 'open'
                        ? 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400'
                        : pr.state === 'merged'
                        ? 'bg-purple-100 text-purple-800 dark:bg-purple-900/30 dark:text-purple-400'
                        : 'bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-400'
                    ]"
                  >
                    {{ pr.state }}
                  </span>
                  <span class="font-semibold text-gray-700 dark:text-gray-300">#{{ pr.number }}</span>
                </div>
                <p class="mt-1 text-sm text-gray-800 dark:text-gray-200">
                  {{ pr.title }}
                </p>
                <div class="mt-1 flex items-center space-x-3 text-xs text-gray-500 dark:text-gray-400">
                  <span>{{ pr.source_branch }} → {{ pr.target_branch }}</span>
                  <span>by {{ pr.author_username }}</span>
                </div>
              </div>
              <svg class="ml-2 h-4 w-4 flex-shrink-0 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"></path>
              </svg>
            </div>
          </a>
        </div>
      </div>
    </div>

    <div v-if="error" class="mt-4 rounded-lg border border-red-200 bg-red-50 p-4 dark:border-red-800 dark:bg-red-900/20">
      <p class="text-sm text-red-700 dark:text-red-400">{{ error }}</p>
    </div>
  </div>
</template>

<script>
import { getTaskGitActivity } from '@/actions/tmgr/github';

export default {
  name: 'TaskGitActivity',
  props: {
    taskId: {
      type: [Number, String],
      required: true,
    },
    categoryCode: {
      type: String,
      default: 'TASK',
    },
  },
  data() {
    return {
      activity: {
        commits: [],
        branches: [],
        pull_requests: [],
      },
      loading: true,
      error: null,
    };
  },
  computed: {
    hasActivity() {
      return (
        this.activity.commits.length > 0 ||
        this.activity.branches.length > 0 ||
        this.activity.pull_requests.length > 0
      );
    },
  },
  async mounted() {
    await this.loadActivity();
  },
  watch: {
    hasActivity: {
      handler(newVal) {
        const count = 
          (this.activity.commits?.length || 0) + 
          (this.activity.branches?.length || 0) + 
          (this.activity.pull_requests?.length || 0);
        this.$emit('update:count', count);
      },
      immediate: true
    }
  },
  methods: {
    async loadActivity() {
      try {
        this.loading = true;
        this.error = null;
        this.activity = await getTaskGitActivity(this.taskId);
      } catch (e) {
        console.error('Failed to load git activity:', e);
        if (e.response?.status !== 404) {
          this.error = 'Failed to load git activity';
        }
      } finally {
        this.loading = false;
      }
    },
    formatDate(dateString) {
      const date = new Date(dateString);
      const now = new Date();
      const diffMs = now - date;
      const diffMins = Math.floor(diffMs / 60000);
      const diffHours = Math.floor(diffMs / 3600000);
      const diffDays = Math.floor(diffMs / 86400000);

      if (diffMins < 1) return 'just now';
      if (diffMins < 60) return `${diffMins}m ago`;
      if (diffHours < 24) return `${diffHours}h ago`;
      if (diffDays < 7) return `${diffDays}d ago`;
      
      return date.toLocaleDateString();
    },
  },
};
</script>
