<template>
  <div>
    <div v-if="loading" class="flex items-center justify-center py-8">
      <div class="h-8 w-8 animate-spin rounded-full border-4 border-violet-500 border-t-transparent"></div>
    </div>

    <div v-else class="space-y-4">
      <div class="rounded-lg border border-gray-300 bg-gray-50 p-4 dark:border-gray-600 dark:bg-gray-800">
        <button
          @click="showLaunchForm = !showLaunchForm"
          class="flex w-full items-center justify-between text-sm font-medium text-gray-700 dark:text-gray-300"
        >
          <span>Launch New Agent</span>
          <svg
            :class="['h-5 w-5 transition-transform', showLaunchForm ? 'rotate-180' : '']"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"></path>
          </svg>
        </button>
        
        <div v-if="showLaunchForm" class="mt-4">
          <div v-if="loadingBranches" class="flex items-center justify-center py-4">
            <div class="h-6 w-6 animate-spin rounded-full border-3 border-violet-500 border-t-transparent"></div>
          </div>
          
          <div v-else-if="branches.length || branchSearch" class="space-y-4">
            <div>
              <label class="mb-2 block text-sm font-medium text-gray-700 dark:text-gray-300">
                Source Branch
              </label>
              <input
                v-model="branchSearch"
                @input="onBranchSearch"
                type="text"
                placeholder="Search branches..."
                class="mb-2 w-full rounded border border-gray-300 bg-white px-3 py-2 text-sm focus:border-violet-500 focus:outline-none focus:ring-1 focus:ring-violet-500 dark:border-gray-600 dark:bg-gray-900 dark:text-white"
              />
              <div v-if="loadingBranches" class="flex items-center justify-center py-2">
                <div class="h-4 w-4 animate-spin rounded-full border-2 border-violet-500 border-t-transparent"></div>
                <span class="ml-2 text-xs text-gray-500">Searching...</span>
              </div>
              <template v-else-if="branches.length">
                <select
                  v-model="selectedBranch"
                  :size="Math.min(branches.length, 8)"
                  class="w-full rounded border border-gray-300 bg-white px-3 py-2 text-sm focus:border-violet-500 focus:outline-none focus:ring-1 focus:ring-violet-500 dark:border-gray-600 dark:bg-gray-900 dark:text-white"
                >
                  <option v-for="branch in branches" :key="branch.name" :value="branch.name">
                    {{ branch.name }}{{ branch.protected ? ' 🔒' : '' }}
                  </option>
                </select>
                <p class="mt-1 text-xs text-gray-500 dark:text-gray-400">
                  {{ branches.length }} branch{{ branches.length !== 1 ? 'es' : '' }} found
                </p>
              </template>
              <div v-else class="py-4 text-center">
                <p class="text-sm text-gray-500 dark:text-gray-400">
                  No branches found for "{{ branchSearch }}"
                </p>
                <button
                  @click="branchSearch = ''; loadBranchesFromGitHub()"
                  class="mt-2 text-sm text-violet-600 hover:underline dark:text-violet-400"
                >
                  Clear search
                </button>
              </div>
            </div>
            
            <button
              @click="launchAgent"
              :disabled="launching || !selectedBranch"
              class="flex w-full items-center justify-center gap-2 rounded bg-violet-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-violet-600 disabled:opacity-50"
            >
              <svg v-if="!launching" class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"></path>
              </svg>
              <div v-else class="h-4 w-4 animate-spin rounded-full border-2 border-white border-t-transparent"></div>
              {{ launching ? 'Launching...' : 'Launch Cursor Agent' }}
            </button>
          </div>
          
          <div v-else class="text-center">
            <p class="mb-2 text-sm text-red-600 dark:text-red-400">
              {{ branchesError || 'Failed to load branches' }}
            </p>
            <button
              @click="branchSearch = ''; loadBranchesFromGitHub()"
              class="text-sm text-violet-600 hover:underline dark:text-violet-400"
            >
              Try again
            </button>
          </div>
        </div>
      </div>

      <div v-if="!agents.length" class="rounded-lg border border-gray-200 bg-white p-8 text-center dark:border-gray-700 dark:bg-gray-800">
        <svg class="mx-auto mb-3 h-12 w-12 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"></path>
        </svg>
        <p class="text-gray-600 dark:text-gray-400">
          No agents launched yet
        </p>
      </div>

      <div v-else class="space-y-3">
        <h3 class="text-sm font-medium text-gray-700 dark:text-gray-300">
          Agent History <span class="text-xs text-gray-500 dark:text-gray-400">({{ agents.length }})</span>
        </h3>
        
        <div
          v-for="agent in agents"
          :key="agent.id"
          class="rounded-lg border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800"
        >
        <div class="mb-3 flex items-center justify-between">
          <div class="flex items-center gap-2">
            <span
              :class="[
                'rounded px-2 py-1 text-xs font-semibold',
                agent.status === 'RUNNING'
                  ? 'bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-400'
                  : agent.status === 'FINISHED'
                  ? 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400'
                  : agent.status === 'FAILED'
                  ? 'bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-400'
                  : 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-300'
              ]"
            >
              {{ agent.status }}
            </span>
            <button
              v-if="agent.status === 'CREATING' || agent.status === 'RUNNING'"
              @click="loadAgents"
              class="rounded p-1 text-gray-500 transition hover:bg-gray-100 hover:text-gray-700 dark:text-gray-400 dark:hover:bg-gray-700 dark:hover:text-gray-200"
              title="Refresh status"
            >
              <svg class="h-3 w-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"></path>
              </svg>
            </button>
            <span class="text-sm text-gray-600 dark:text-gray-400">
              {{ formatDate(agent.created_at) }}
            </span>
          </div>

          <button
            v-if="agent.status === 'RUNNING'"
            @click="stopAgent(agent.id)"
            :disabled="stopping"
            class="rounded bg-red-500 px-3 py-1 text-xs font-medium text-white transition hover:bg-red-600 disabled:opacity-50"
          >
            {{ stopping ? 'Stopping...' : 'Stop' }}
          </button>
        </div>

        <div v-if="agent.branch_name" class="mb-2 text-sm text-gray-700 dark:text-gray-300">
          <span class="font-medium">Branch:</span>
          <code class="ml-1 rounded bg-gray-100 px-2 py-0.5 text-xs dark:bg-gray-700">{{ agent.branch_name }}</code>
        </div>

        <div v-if="agent.summary" class="mb-2 text-sm text-gray-700 dark:text-gray-300">
          <span class="font-medium">Summary:</span>
          <p class="mt-1">{{ agent.summary }}</p>
        </div>

        <div v-if="agent.pr_url" class="mb-3">
          <a
            :href="agent.pr_url"
            target="_blank"
            class="inline-flex items-center gap-1 text-sm text-violet-600 hover:underline dark:text-violet-400"
          >
            <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"></path>
            </svg>
            View Pull Request
          </a>
        </div>

        <div v-if="agent.error_message" class="rounded border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-800 dark:bg-red-900/20 dark:text-red-400">
          {{ agent.error_message }}
        </div>

        <div v-if="agent.status === 'RUNNING'" class="mt-4 space-y-2">
          <label class="block text-sm font-medium text-gray-700 dark:text-gray-300">
            Send Follow-up Instruction
          </label>
          <div class="flex gap-2">
            <input
              v-model="followUpPrompt"
              type="text"
              placeholder="Type additional instructions..."
              class="flex-1 rounded border border-gray-300 bg-white px-3 py-2 text-sm focus:border-violet-500 focus:outline-none focus:ring-1 focus:ring-violet-500 dark:border-gray-600 dark:bg-gray-900 dark:text-white"
              @keyup.enter="sendFollowUpToAgent(agent.id)"
            />
            <button
              @click="sendFollowUpToAgent(agent.id)"
              :disabled="!followUpPrompt || sendingFollowUp"
              class="rounded bg-violet-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-violet-600 disabled:opacity-50"
            >
              {{ sendingFollowUp ? 'Sending...' : 'Send' }}
            </button>
          </div>
        </div>
        </div>
      </div>

      <div v-if="error" class="rounded border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-800 dark:bg-red-900/20 dark:text-red-400">
        {{ error }}
      </div>
    </div>
  </div>
</template>

<script>
import { defineComponent } from 'vue';
import {
  launchCursorAgent,
  getCursorAgents,
  stopCursorAgent,
  sendFollowUp,
  getGitHubBranches,
} from '@/actions/tmgr/cursor';

export default defineComponent({
  name: 'TaskCursorAgent',
  props: {
    taskId: {
      type: Number,
      required: true,
    },
  },
  data() {
    return {
      agents: [],
      branches: [],
      selectedBranch: null,
      branchSearch: '',
      searchTimeout: null,
      loading: true,
      loadingBranches: false,
      launching: false,
      stopping: false,
      sendingFollowUp: false,
      followUpPrompt: '',
      error: null,
      branchesError: null,
      pollingInterval: null,
      showLaunchForm: false,
    };
  },
  computed: {
    hasActiveAgents() {
      return this.agents.some(agent => 
        agent.status === 'CREATING' || agent.status === 'RUNNING'
      );
    },
  },
  watch: {
    agents: {
      handler(newAgents) {
        const hasActive = newAgents.some(agent => 
          agent.status === 'CREATING' || agent.status === 'RUNNING'
        );
        
        if (hasActive && !this.pollingInterval) {
          console.log('Starting polling - found active agents');
          this.startPolling();
        } else if (!hasActive && this.pollingInterval) {
          console.log('Stopping polling - no active agents');
          this.stopPolling();
        }
      },
      deep: true,
    },
  },
  async mounted() {
    await this.loadAgents();
    await this.loadBranchesFromGitHub();
    
    if (!this.agents.length) {
      this.showLaunchForm = true;
    }
  },
  beforeUnmount() {
    this.stopPolling();
  },
  methods: {
    async loadAgents() {
      try {
        this.loading = true;
        this.error = null;
        this.agents = await getCursorAgents(this.taskId);
      } catch (e) {
        console.error('Failed to load Cursor agents:', e);
        if (e.response?.status !== 404) {
          this.error = 'Failed to load agents';
        }
      } finally {
        this.loading = false;
      }
    },
    async loadBranchesFromGitHub(search = null) {
      try {
        this.loadingBranches = true;
        this.branchesError = null;
        this.branches = await getGitHubBranches(this.taskId, search);
        
        if (this.branches.length > 0 && !this.selectedBranch) {
          this.selectedBranch = this.branches.find(b => b.name === 'master' || b.name === 'main')?.name || this.branches[0].name;
        }
        
        if (this.branches.length > 0 && !this.branches.find(b => b.name === this.selectedBranch)) {
          this.selectedBranch = this.branches[0].name;
        }
      } catch (e) {
        console.error('Failed to load branches:', e);
        this.branchesError = e.response?.data?.error || 'Failed to load branches from GitHub';
      } finally {
        this.loadingBranches = false;
      }
    },
    onBranchSearch() {
      if (this.searchTimeout) {
        clearTimeout(this.searchTimeout);
      }
      
      this.searchTimeout = setTimeout(() => {
        this.loadBranchesFromGitHub(this.branchSearch || undefined);
      }, 300);
    },
    async launchAgent() {
      try {
        this.launching = true;
        this.error = null;
        const agent = await launchCursorAgent(this.taskId, this.selectedBranch);
        this.agents.unshift(agent);
        this.showLaunchForm = false;
        this.$emit('launched', agent);
      } catch (e) {
        console.error('Failed to launch agent:', e);
        this.error = e.response?.data?.error || 'Failed to launch agent';
      } finally {
        this.launching = false;
      }
    },
    async stopAgent(agentId) {
      try {
        this.stopping = true;
        await stopCursorAgent(this.taskId, agentId);
        await this.loadAgents();
        this.$emit('stopped');
      } catch (e) {
        console.error('Failed to stop agent:', e);
        this.error = e.response?.data?.error || 'Failed to stop agent';
      } finally {
        this.stopping = false;
      }
    },
    async sendFollowUpToAgent(agentId) {
      if (!this.followUpPrompt) return;

      try {
        this.sendingFollowUp = true;
        await sendFollowUp(this.taskId, agentId, this.followUpPrompt);
        this.followUpPrompt = '';
        this.$emit('followup-sent');
      } catch (e) {
        console.error('Failed to send follow-up:', e);
        this.error = e.response?.data?.error || 'Failed to send follow-up';
      } finally {
        this.sendingFollowUp = false;
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
    startPolling() {
      if (this.pollingInterval) {
        console.log('Polling already active');
        return;
      }
      
      console.log('Starting polling every 5 seconds');
      
      this.pollingInterval = setInterval(async () => {
        console.log('Polling for agent status updates...');
        try {
          const agents = await getCursorAgents(this.taskId);
          console.log('Got agents:', agents);
          this.agents = agents;
        } catch (e) {
          console.error('Failed to poll agents:', e);
        }
      }, 5000);
    },
    stopPolling() {
      if (this.pollingInterval) {
        clearInterval(this.pollingInterval);
        this.pollingInterval = null;
      }
    },
  },
});
</script>
