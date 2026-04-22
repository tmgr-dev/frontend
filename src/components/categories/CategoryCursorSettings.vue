<template>
  <div v-if="!isCreate" class="mt-6 border-t border-line pt-6">
    <h3 class="mb-4 text-base font-semibold text-ink">
      Cursor AI Integration
    </h3>

    <div v-if="loading" class="flex items-center justify-center py-8">
      <div class="h-8 w-8 animate-spin rounded-full border-4 border-brand border-t-transparent"></div>
    </div>

    <div v-else-if="!configured" class="rounded-card border border-line bg-surface-sunken p-6 shadow-tmgr-xs">
      <div class="mb-4 flex justify-center">
        <div class="flex h-12 w-12 items-center justify-center rounded-pill bg-surface text-ink-subtle">
          <svg class="h-6 w-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"></path>
          </svg>
        </div>
      </div>
      <h4 class="mb-2 text-center text-base font-semibold text-ink">
        Configure Cursor AI Agent
      </h4>
      <p class="mx-auto mb-5 max-w-md text-center text-sm text-ink-muted">
        Add your Cursor API key to enable AI-powered code generation for this category
      </p>

      <div class="mt-4 space-y-3">
        <div>
          <label for="cursorApiKey" class="mb-1.5 block text-sm font-semibold text-ink">
            Cursor API Key
          </label>
          <input
            id="cursorApiKey"
            v-model="apiKey"
            type="password"
            placeholder="cur_xxxxxxxxxxxxxxxx"
            class="h-10 w-full rounded-card border border-line bg-surface px-3 font-mono text-sm text-ink outline-none placeholder:text-ink-faint focus:border-line-strong"
          />
          <p class="mt-1.5 text-xs text-ink-muted">
            Get your API key from
            <a href="https://cursor.com/dashboard?tab=cloud-agents" target="_blank" class="text-brand-fg hover:underline">
              Cursor Dashboard
            </a>
          </p>
        </div>

        <div v-if="error" class="rounded-card border border-status-fix/40 bg-status-fix-bg p-3 text-sm text-status-fix-fg">
          {{ error }}
        </div>

        <button
          @click="saveKey"
          :disabled="!apiKey || saving"
          class="h-10 w-full rounded-pill bg-brand text-sm font-semibold text-white shadow-tmgr-xs transition-colors hover:bg-brand-hover disabled:opacity-50"
        >
          {{ saving ? 'Validating…' : 'Save API Key' }}
        </button>
      </div>
    </div>

    <div v-else class="space-y-4">
      <div class="rounded-card border border-brand/30 bg-brand-bg p-4">
        <div class="flex items-center justify-between gap-3">
          <div>
            <p class="text-sm font-semibold text-brand-fg">
              Cursor AI Agent Configured
            </p>
            <p class="mt-1 text-xs text-brand-fg/80">
              Ready to assist with code generation for this category
            </p>
          </div>
          <svg class="h-5 w-5 shrink-0 text-brand-fg" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
          </svg>
        </div>
      </div>

      <div class="flex flex-wrap gap-2">
        <button
          @click="editing = true"
          class="inline-flex h-9 items-center justify-center rounded-pill border border-line bg-surface px-4 text-sm font-semibold text-ink transition-colors hover:bg-surface-hover"
        >
          Update Key
        </button>
        <button
          @click="removeKey"
          :disabled="removing"
          class="inline-flex h-9 items-center justify-center rounded-pill border border-status-fix/40 bg-status-fix-bg px-4 text-sm font-semibold text-status-fix-fg transition-colors hover:opacity-90 disabled:opacity-50"
        >
          {{ removing ? 'Removing…' : 'Remove Key' }}
        </button>
      </div>

      <div v-if="editing" class="mt-4 space-y-3 rounded-card border border-line bg-surface-sunken p-4">
        <div>
          <label for="cursorApiKeyEdit" class="mb-1.5 block text-sm font-semibold text-ink">
            New Cursor API Key
          </label>
          <input
            id="cursorApiKeyEdit"
            v-model="apiKey"
            type="password"
            placeholder="cur_xxxxxxxxxxxxxxxx"
            class="h-10 w-full rounded-card border border-line bg-surface px-3 font-mono text-sm text-ink outline-none placeholder:text-ink-faint focus:border-line-strong"
          />
        </div>

        <div v-if="error" class="rounded-card border border-status-fix/40 bg-status-fix-bg p-3 text-sm text-status-fix-fg">
          {{ error }}
        </div>

        <div class="flex gap-2">
          <button
            @click="saveKey"
            :disabled="!apiKey || saving"
            class="inline-flex h-9 items-center justify-center rounded-pill bg-brand px-4 text-sm font-semibold text-white shadow-tmgr-xs transition-colors hover:bg-brand-hover disabled:opacity-50"
          >
            {{ saving ? 'Validating…' : 'Save' }}
          </button>
          <button
            @click="cancelEdit"
            class="inline-flex h-9 items-center justify-center rounded-pill border border-line bg-surface px-4 text-sm font-semibold text-ink transition-colors hover:bg-surface-hover"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script>
import { defineComponent } from 'vue';
import { updateCursorApiKey, deleteCursorApiKey, getCursorStatus } from '@/actions/tmgr/cursor';

export default defineComponent({
  name: 'CategoryCursorSettings',
  props: {
    categoryId: {
      type: Number,
      required: true,
    },
    isCreate: {
      type: Boolean,
      default: false,
    },
  },
  emits: ['cursor-configured', 'cursor-removed'],
  data() {
    return {
      loading: false,
      configured: false,
      editing: false,
      apiKey: '',
      saving: false,
      removing: false,
      error: null,
    };
  },
  async mounted() {
    if (!this.isCreate) {
      await this.loadStatus();
    }
  },
  methods: {
    async loadStatus() {
      this.loading = true;
      try {
        const status = await getCursorStatus(this.categoryId);
        this.configured = status.configured;
      } catch (e) {
        console.error('Failed to load Cursor status:', e);
        this.configured = false;
      } finally {
        this.loading = false;
      }
    },
    async saveKey() {
      if (!this.apiKey) return;

      this.saving = true;
      this.error = null;

      try {
        await updateCursorApiKey(this.categoryId, this.apiKey);
        this.configured = true;
        this.editing = false;
        this.apiKey = '';
        this.$emit('cursor-configured');
      } catch (e) {
        this.error = e.response?.data?.error || 'Failed to save API key';
      } finally {
        this.saving = false;
      }
    },
    async removeKey() {
      if (!confirm('Are you sure you want to remove Cursor API key from this category?')) {
        return;
      }

      this.removing = true;
      this.error = null;

      try {
        await deleteCursorApiKey(this.categoryId);
        this.configured = false;
        this.editing = false;
        this.apiKey = '';
        this.$emit('cursor-removed');
      } catch (e) {
        this.error = e.response?.data?.error || 'Failed to remove API key';
      } finally {
        this.removing = false;
      }
    },
    cancelEdit() {
      this.editing = false;
      this.apiKey = '';
      this.error = null;
    },
  },
});
</script>
