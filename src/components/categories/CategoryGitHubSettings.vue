<template>
  <div v-if="!isCreate" class="mt-6 border-t border-line pt-6">
    <h3 class="mb-4 text-base font-semibold text-ink">
      GitHub Integration
    </h3>

    <div v-if="loading" class="flex items-center justify-center py-8">
      <div class="h-8 w-8 animate-spin rounded-full border-4 border-brand border-t-transparent"></div>
    </div>

    <div v-else-if="!status.has_installation" class="rounded-card border border-line bg-surface-sunken p-6 shadow-tmgr-xs">
      <div class="mb-4 flex justify-center">
        <div class="flex h-12 w-12 items-center justify-center rounded-pill bg-surface text-ink-subtle">
          <svg class="h-6 w-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1"></path>
          </svg>
        </div>
      </div>
      <h4 class="mb-2 text-center text-base font-semibold text-ink">
        Connect GitHub Repository
      </h4>
      <p class="mx-auto mb-5 max-w-md text-center text-sm text-ink-muted">
        Link your GitHub repository to automatically track commits, branches, and pull requests
      </p>
      <button
        @click="installGitHubApp"
        :disabled="installing"
        class="mx-auto flex h-10 items-center justify-center rounded-pill bg-brand px-6 text-sm font-semibold text-white shadow-tmgr-xs transition-colors hover:bg-brand-hover disabled:opacity-50"
      >
        <span v-if="installing">Installing…</span>
        <span v-else>Install GitHub App</span>
      </button>
    </div>

    <div v-else-if="!status.repository" class="space-y-4">
      <div class="rounded-card border border-status-done/40 bg-status-done-bg p-4">
        <p class="text-sm text-status-done-fg">
          ✓ GitHub App is installed. Select a repository to connect:
        </p>
      </div>

      <div v-if="status.available_repositories.length === 0" class="rounded-card border border-status-fix/40 bg-status-fix-bg p-4">
        <p class="text-sm text-status-fix-fg">
          No repositories available. Make sure the GitHub App has access to your repositories.
        </p>
      </div>

      <div v-else>
        <label class="mb-2 block text-sm font-semibold text-ink">
          Select Repository
        </label>
        <select
          v-model="selectedRepo"
          class="h-10 w-full rounded-card border border-line bg-surface px-3 text-sm text-ink outline-none focus:border-line-strong"
        >
          <option :value="null">— Select repository —</option>
          <option
            v-for="repo in status.available_repositories"
            :key="repo.id"
            :value="repo"
          >
            {{ repo.full_name }}
          </option>
        </select>

        <button
          @click="connectRepository"
          :disabled="!selectedRepo || connecting"
          class="mt-4 inline-flex h-10 items-center justify-center rounded-pill bg-brand px-6 text-sm font-semibold text-white shadow-tmgr-xs transition-colors hover:bg-brand-hover disabled:cursor-not-allowed disabled:opacity-50"
        >
          <span v-if="connecting">Connecting…</span>
          <span v-else>Connect Repository</span>
        </button>
      </div>
    </div>

    <div v-else class="space-y-4">
      <div class="rounded-card border border-status-done/40 bg-status-done-bg p-4">
        <div class="flex items-center justify-between gap-3">
          <div>
            <p class="text-sm font-semibold text-status-done-fg">
              Connected to
              <a
                :href="`https://github.com/${status.repository.full_name}`"
                target="_blank"
                class="underline hover:opacity-80"
              >
                {{ status.repository.full_name }}
              </a>
            </p>
            <p v-if="status.repository.last_synced_at" class="mt-1 text-xs text-status-done-fg/80">
              Last synced: {{ formatDate(status.repository.last_synced_at) }}
            </p>
          </div>
          <svg class="h-5 w-5 shrink-0 text-status-done-fg" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
          </svg>
        </div>
      </div>

      <div class="flex flex-wrap gap-2">
        <button
          @click="syncRepository"
          :disabled="syncing"
          class="inline-flex h-9 items-center justify-center rounded-pill bg-brand px-4 text-sm font-semibold text-white shadow-tmgr-xs transition-colors hover:bg-brand-hover disabled:opacity-50"
        >
          <span v-if="syncing">Syncing…</span>
          <span v-else>Sync Now</span>
        </button>

        <button
          @click="relinkRepository"
          :disabled="relinking"
          class="inline-flex h-9 items-center justify-center rounded-pill border border-line bg-surface px-4 text-sm font-semibold text-ink transition-colors hover:bg-surface-hover disabled:opacity-50"
          title="Relink existing commits/branches to tasks"
        >
          <span v-if="relinking">Relinking…</span>
          <span v-else>Relink Tasks</span>
        </button>

        <button
          @click="disconnectRepository"
          :disabled="disconnecting"
          class="inline-flex h-9 items-center justify-center rounded-pill border border-status-fix/40 bg-status-fix-bg px-4 text-sm font-semibold text-status-fix-fg transition-colors hover:opacity-90 disabled:opacity-50"
        >
          <span v-if="disconnecting">Disconnecting…</span>
          <span v-else>Disconnect</span>
        </button>
      </div>
    </div>

    <div v-if="error" class="mt-4 rounded-card border border-status-fix/40 bg-status-fix-bg p-4">
      <p class="text-sm text-status-fix-fg">{{ error }}</p>
    </div>
  </div>
</template>

<script>
import {
  getGitHubInstallUrl,
  syncGitHubInstallations,
  getCategoryGitHubStatus,
  connectCategoryRepository,
  disconnectCategoryRepository,
  syncCategoryRepository,
  relinkCategoryRepository,
} from '@/actions/tmgr/github';

export default {
  name: 'CategoryGitHubSettings',
  props: {
    categoryId: {
      type: [Number, String],
      required: true,
    },
    isCreate: {
      type: Boolean,
      default: false,
    },
  },
  data() {
    return {
      status: {
        has_installation: false,
        repository: null,
        available_repositories: [],
      },
      selectedRepo: null,
      loading: true,
      installing: false,
      connecting: false,
      disconnecting: false,
      syncing: false,
      relinking: false,
      error: null,
    };
  },
  async mounted() {
    if (!this.isCreate) {
      await this.syncInstallations();
      await this.loadStatus();
    }
  },
  methods: {
    async syncInstallations() {
      try {
        await syncGitHubInstallations();
      } catch (e) {
        console.error('Failed to sync GitHub installations:', e);
      }
    },
    async loadStatus() {
      try {
        this.loading = true;
        this.error = null;
        this.status = await getCategoryGitHubStatus(this.categoryId);
      } catch (e) {
        console.error('Failed to load GitHub status:', e);
        this.error = 'Failed to load GitHub integration status';
      } finally {
        this.loading = false;
      }
    },
    async installGitHubApp() {
      try {
        this.installing = true;
        this.error = null;
        const { url } = await getGitHubInstallUrl();
        window.location.href = url;
      } catch (e) {
        console.error('Failed to get install URL:', e);
        this.error = 'Failed to start GitHub App installation';
        this.installing = false;
      }
    },
    async connectRepository() {
      if (!this.selectedRepo) return;

      try {
        this.connecting = true;
        this.error = null;

        await connectCategoryRepository(this.categoryId, {
          installation_id: this.selectedRepo.installation_id,
          github_repo_id: this.selectedRepo.id,
          full_name: this.selectedRepo.full_name,
        });

        this.$emit('connected', this.selectedRepo.full_name);
        await this.loadStatus();
      } catch (e) {
        console.error('Failed to connect repository:', e);
        this.error = e.response?.data?.error || 'Failed to connect repository';
      } finally {
        this.connecting = false;
      }
    },
    async disconnectRepository() {
      if (!confirm('Are you sure you want to disconnect this repository?')) {
        return;
      }

      try {
        this.disconnecting = true;
        this.error = null;

        await disconnectCategoryRepository(this.categoryId);

        this.$emit('disconnected');
        await this.loadStatus();
      } catch (e) {
        console.error('Failed to disconnect repository:', e);
        this.error = e.response?.data?.error || 'Failed to disconnect repository';
      } finally {
        this.disconnecting = false;
      }
    },
    async syncRepository() {
      try {
        this.syncing = true;
        this.error = null;

        const result = await syncCategoryRepository(this.categoryId);

        this.$emit('synced');
        await this.loadStatus();
      } catch (e) {
        console.error('Failed to sync repository:', e);
        this.error = e.response?.data?.error || 'Failed to sync repository';
      } finally {
        this.syncing = false;
      }
    },
    async relinkRepository() {
      try {
        this.relinking = true;
        this.error = null;

        const result = await relinkCategoryRepository(this.categoryId);

        this.$emit('relinked', result.stats);
        await this.loadStatus();
      } catch (e) {
        console.error('Failed to relink repository:', e);
        this.error = e.response?.data?.error || 'Failed to relink task references';
      } finally {
        this.relinking = false;
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
      if (diffMins < 60) return `${diffMins} minute${diffMins > 1 ? 's' : ''} ago`;
      if (diffHours < 24) return `${diffHours} hour${diffHours > 1 ? 's' : ''} ago`;
      if (diffDays < 7) return `${diffDays} day${diffDays > 1 ? 's' : ''} ago`;
      
      return date.toLocaleDateString();
    },
  },
};
</script>
