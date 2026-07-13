<template>
	<div class="space-y-4">
		<div class="flex flex-wrap gap-3">
			<button
				type="button"
				class="w-44 rounded-lg border border-gray-200 p-3 text-left transition-colors dark:border-gray-700"
				:class="
					selected === 'default'
						? 'ring-2 ring-blue-500'
						: 'hover:bg-gray-50 dark:hover:bg-gray-800'
				"
				@click="choose('default')"
			>
				<div class="mb-2 flex gap-1">
					<span class="h-5 flex-1 rounded bg-white border border-gray-200"></span>
					<span class="h-5 flex-1 rounded bg-gray-800"></span>
					<span class="h-5 flex-1 rounded bg-blue-500"></span>
				</div>
				<div class="text-sm font-semibold text-gray-900 dark:text-gray-100">
					Default
				</div>
				<div class="text-xs text-gray-500 dark:text-gray-400">
					light / dark toggle
				</div>
			</button>

			<button
				v-for="t in registry"
				:key="t.id"
				type="button"
				class="w-44 rounded-lg border p-3 text-left transition-transform hover:-translate-y-0.5"
				:class="selected === t.id ? 'ring-2 ring-blue-500' : ''"
				:style="{ background: t.tokens.bg, borderColor: t.tokens.border }"
				@click="choose(t.id)"
			>
				<div class="mb-2 flex gap-1">
					<span
						class="h-5 flex-1 rounded"
						:style="{ background: t.tokens.surface }"
					></span>
					<span
						class="h-5 flex-1 rounded"
						:style="{ background: t.tokens.border }"
					></span>
					<span
						class="h-5 flex-1 rounded"
						:style="{ background: t.tokens.brand }"
					></span>
					<span
						class="h-5 flex-1 rounded"
						:style="{ background: t.tokens.timer }"
					></span>
				</div>
				<div class="text-sm font-semibold" :style="{ color: t.tokens.text }">
					{{ t.name }}
				</div>
			</button>
		</div>

		<div v-if="selected === 'default'" class="flex items-center gap-3">
			<span class="text-sm text-gray-600 dark:text-gray-300">Mode:</span>
			<button
				type="button"
				class="rounded-full border border-gray-200 px-3 py-1 text-sm dark:border-gray-700"
				:class="colorScheme === 'default' ? 'bg-blue-500 text-white' : ''"
				@click="chooseScheme('default')"
			>
				Light
			</button>
			<button
				type="button"
				class="rounded-full border border-gray-200 px-3 py-1 text-sm dark:border-gray-700"
				:class="colorScheme === 'dark' ? 'bg-blue-500 text-white' : ''"
				@click="chooseScheme('dark')"
			>
				Dark
			</button>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, computed } from 'vue';
	import store from '@/store';
	import { THEME_REGISTRY } from '@/theme/registry';
	import { persistThemeSettings } from '@/actions/tmgr/user';

	export default defineComponent({
		name: 'ThemePicker',
		setup() {
			const registry = THEME_REGISTRY;
			const selected = computed(() => store.state.theme || 'default');
			const colorScheme = computed(() => store.state.colorScheme || 'default');

			async function persist() {
				try {
					if (store.getters.isLoggedIn) {
						await persistThemeSettings(store.state.theme, store.state.colorScheme);
					}
				} catch (e) {
					console.error('Failed to persist theme', e);
				}
			}
			async function choose(id: string) {
				store.commit('setTheme', id);
				await persist();
			}
			async function chooseScheme(scheme: string) {
				store.commit('setColorScheme', scheme);
				await persist();
			}
			return { registry, selected, colorScheme, choose, chooseScheme };
		},
	});
</script>
