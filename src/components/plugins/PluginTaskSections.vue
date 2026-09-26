<template>
	<div v-if="sections.length" class="flex flex-col gap-4">
		<section
			v-for="section in sections"
			:key="section.key"
			class="rounded-md border border-border p-3"
		>
			<header class="mb-2 flex items-center gap-2">
				<Plug class="h-3.5 w-3.5 text-muted-foreground" />
				<h3 class="text-sm font-semibold text-foreground">
					{{ section.title }}
				</h3>
				<span class="text-2xs text-muted-foreground">{{
					section.pluginName
				}}</span>
			</header>
			<PluginView
				v-if="trees[section.key]"
				:node="trees[section.key]!"
				:plugin-id="section.pluginId"
			/>
			<p v-else-if="errors[section.key]" class="text-xs text-muted-foreground">
				{{ errors[section.key] }}
			</p>
		</section>
	</div>
</template>

<script lang="ts">
	import PluginView from '@/components/plugins/PluginView.vue';
	import { taskSnapshot } from '@/pluginSystem/cardBadges';
	import { pluginHost, pluginState } from '@/pluginSystem/state';
	import type { UiNode } from '@/pluginSystem/uiTree';
	import { Plug } from 'lucide-vue-next';
	import { computed, defineComponent, reactive, watch } from 'vue';

	export default defineComponent({
		name: 'PluginTaskSections',
		components: { PluginView, Plug },
		props: {
			task: { type: Object, required: true },
		},
		setup(props) {
			const sections = computed(() =>
				Object.values(pluginState.plugins)
					.filter((plugin) => plugin.status === 'running')
					.flatMap((plugin) =>
						plugin.manifest.contributes.taskPanelSections.map((section) => ({
							key: `${plugin.manifest.id}/${section.id}`,
							pluginId: plugin.manifest.id,
							pluginName: plugin.manifest.name,
							id: section.id,
							title: section.title,
						})),
					),
			);
			const trees = reactive<Record<string, UiNode | null>>({});
			const errors = reactive<Record<string, string>>({});
			let request = 0;

			const render = async () => {
				const current = ++request;
				const host = pluginHost();
				if (!host || !props.task?.id) return;
				const snapshot = taskSnapshot(props.task);
				await Promise.all(
					sections.value.map(async (section) => {
						try {
							const tree = await host.renderSection(
								section.pluginId,
								section.id,
								snapshot,
							);
							if (current !== request) return;
							trees[section.key] = tree;
							delete errors[section.key];
						} catch (error) {
							if (current !== request) return;
							trees[section.key] = null;
							errors[section.key] = `Not available: ${
								error instanceof Error ? error.message : error
							}`;
						}
					}),
				);
			};

			watch(
				() => {
					const t = props.task as Record<string, any>;
					return `${sections.value.map((s) => s.key).join()}|${
						pluginState.revision
					}:${sections.value
						.map((s) => pluginState.revisions[s.pluginId] ?? 0)
						.join(':')}|${t?.id}:${t?.common_time}:${t?.approximately_time}:${
						t?.start_time
					}`;
				},
				() => void render(),
				{ immediate: true },
			);

			return { sections, trees, errors };
		},
	});
</script>
