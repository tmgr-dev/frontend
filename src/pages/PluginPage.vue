<template>
	<div class="container max-w-5xl py-4">
		<div v-if="!entry" class="text-sm text-muted-foreground">
			This plugin is not installed.
		</div>
		<div
			v-else-if="entry.status !== 'running'"
			class="flex flex-col gap-2 text-sm text-muted-foreground"
		>
			<p>
				{{ entry.manifest.name }} is not running in this workspace{{
					entry.error ? `: ${entry.error}` : '.'
				}}
			</p>
			<router-link
				to="/settings/plugins"
				class="w-fit text-primary hover:underline"
			>
				Open plugin settings
			</router-link>
		</div>
		<div v-else-if="view?.ui" class="flex flex-col items-start gap-3 text-sm">
			<p class="text-muted-foreground">
				“{{ view.title }}” is the plugin's own page. It opens in a separate
				window, so it cannot slow down or freeze the app.
			</p>
			<Button size="sm" @click="openWindow">Open window</Button>
		</div>
		<p v-else-if="error" class="text-sm text-red-600 dark:text-red-400">
			{{ error }}
		</p>
		<PluginView v-else-if="tree" :node="tree" :plugin-id="pluginId" />
		<p v-else class="text-sm text-muted-foreground">Loading…</p>
	</div>
</template>

<script lang="ts">
	import PluginView from '@/components/plugins/PluginView.vue';
	import { Button } from '@/components/ui/button';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { pluginHost, pluginState } from '@/pluginSystem/state';
	import type { UiNode } from '@/pluginSystem/uiTree';
	import { computed, defineComponent, ref, watch } from 'vue';
	import { useRoute } from 'vue-router';

	export default defineComponent({
		name: 'PluginPage',
		components: { Button, PluginView },
		setup() {
			const route = useRoute();
			const pluginId = computed(() => String(route.params.pluginId));
			const viewId = computed(() => String(route.params.viewId));
			const entry = computed(() => pluginState.plugins[pluginId.value] ?? null);
			const tree = ref<UiNode | null>(null);
			const view = computed(
				() =>
					entry.value?.manifest.contributes.views.find(
						(v) => v.id === viewId.value,
					) ?? null,
			);
			const openWindow = () =>
				pluginHost()?.openView(pluginId.value, viewId.value);
			const error = ref<string | null>(null);
			let request = 0;
			let openedFor = '';

			const render = async () => {
				const current = ++request;
				if (entry.value?.status !== 'running') return;
				if (view.value?.ui) {
					setDocumentTitle(view.value.title);
					const key = `${pluginId.value}/${viewId.value}`;
					if (openedFor !== key) {
						openedFor = key;
						void openWindow();
					}
					return;
				}
				setDocumentTitle(view.value?.title ?? entry.value.manifest.name);
				try {
					const next = await pluginHost()?.renderPage(
						pluginId.value,
						viewId.value,
					);
					if (current !== request) return;
					tree.value = next ?? null;
					error.value = next ? null : 'The plugin returned nothing to show.';
				} catch (e) {
					if (current !== request) return;
					error.value = `The plugin could not draw this page: ${
						e instanceof Error ? e.message : e
					}`;
				}
			};

			watch(
				() => [
					pluginId.value,
					viewId.value,
					entry.value?.status,
					pluginState.revision,
					pluginState.revisions[pluginId.value],
				],
				() => void render(),
				{ immediate: true },
			);

			return { pluginId, entry, tree, error, view, openWindow };
		},
	});
</script>
