<template>
  <div v-if="!isCreate" class="mt-6 border-t border-gray-200 pt-6 dark:border-gray-700">
    <h3 class="mb-4 text-lg font-bold text-gray-800 dark:text-gray-200">
      Cursor AI Integration
    </h3>

    <div v-if="loading" class="flex items-center justify-center py-8">
      <div class="h-8 w-8 animate-spin rounded-full border-4 border-violet-500 border-t-transparent"></div>
    </div>

    <div v-else-if="!configured" class="rounded-lg border border-gray-300 bg-gray-50 p-6 dark:border-gray-600 dark:bg-gray-800">
      <div class="mb-4">
        <svg class="mx-auto h-12 w-12 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"></path>
        </svg>
      </div>
      <h4 class="mb-2 text-center text-lg font-semibold text-gray-700 dark:text-gray-300">
        Configure Cursor AI Agent
      </h4>
      <p class="mb-4 text-center text-sm text-gray-600 dark:text-gray-400">
        Add your Cursor API key to enable AI-powered code generation for this category
      </p>

      <div class="mt-4 space-y-4">
        <div>
          <label for="cursorApiKey" class="mb-2 block text-sm font-medium text-gray-700 dark:text-gray-300">
            Cursor API Key
          </label>
          <input
            id="cursorApiKey"
            v-model="apiKey"
            type="password"
            placeholder="cur_xxxxxxxxxxxxxxxx"
            class="w-full rounded border border-gray-300 bg-white px-3 py-2 text-sm focus:border-violet-500 focus:outline-none focus:ring-1 focus:ring-violet-500 dark:border-gray-600 dark:bg-gray-900 dark:text-white"
          />
          <p class="mt-1 text-xs text-gray-500 dark:text-gray-400">
            Get your API key from
            <a href="https://cursor.com/dashboard?tab=cloud-agents" target="_blank" class="text-violet-600 hover:underline dark:text-violet-400">
              Cursor Dashboard
            </a>
          </p>
        </div>

        <div v-if="error" class="rounded border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-800 dark:bg-red-900/20 dark:text-red-400">
          {{ error }}
        </div>

        <button
          @click="saveKey"
          :disabled="!apiKey || saving"
          class="w-full rounded bg-violet-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-violet-600 disabled:opacity-50"
        >
          {{ saving ? 'Validating...' : 'Save API Key' }}
        </button>
      </div>
    </div>

    <div v-else class="space-y-4">
      <div class="rounded-lg border border-violet-200 bg-violet-50 p-4 dark:border-violet-800 dark:bg-violet-900/20">
        <div class="flex items-center justify-between">
          <div>
            <p class="font-semibold text-violet-800 dark:text-violet-300">
              Cursor AI Agent Configured
            </p>
            <p class="mt-1 text-xs text-violet-700 dark:text-violet-400">
              Ready to assist with code generation for this category
            </p>
          </div>
          <svg class="h-6 w-6 text-violet-600 dark:text-violet-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
          </svg>
        </div>
      </div>

      <div class="flex gap-2">
        <button
          @click="editing = true"
          class="rounded bg-gray-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-gray-600"
        >
          Update Key
        </button>
        <button
          @click="removeKey"
          :disabled="removing"
          class="rounded bg-red-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-red-600 disabled:opacity-50"
        >
          {{ removing ? 'Removing...' : 'Remove Key' }}
        </button>
      </div>

      <div v-if="editing" class="mt-4 space-y-4 rounded-lg border border-gray-300 bg-white p-4 dark:border-gray-600 dark:bg-gray-800">
        <div>
          <label for="cursorApiKeyEdit" class="mb-2 block text-sm font-medium text-gray-700 dark:text-gray-300">
            New Cursor API Key
          </label>
          <input
            id="cursorApiKeyEdit"
            v-model="apiKey"
            type="password"
            placeholder="cur_xxxxxxxxxxxxxxxx"
            class="w-full rounded border border-gray-300 bg-white px-3 py-2 text-sm focus:border-violet-500 focus:outline-none focus:ring-1 focus:ring-violet-500 dark:border-gray-600 dark:bg-gray-900 dark:text-white"
          />
        </div>

        <div v-if="error" class="rounded border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-800 dark:bg-red-900/20 dark:text-red-400">
          {{ error }}
        </div>

        <div class="flex gap-2">
          <button
            @click="saveKey"
            :disabled="!apiKey || saving"
            class="rounded bg-violet-500 px-4 py-2 text-sm font-medium text-white transition hover:bg-violet-600 disabled:opacity-50"
          >
            {{ saving ? 'Validating...' : 'Save' }}
          </button>
          <button
            @click="cancelEdit"
            class="rounded bg-gray-300 px-4 py-2 text-sm font-medium text-gray-700 transition hover:bg-gray-400 dark:bg-gray-700 dark:text-gray-300 dark:hover:bg-gray-600"
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
