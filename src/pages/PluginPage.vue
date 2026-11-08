<template>
	<PageContainer width="wide">
		<div v-if="!entry" class="text-sm text-muted-foreground">
			{{ $t('plugins.page.notInstalled') }}
		</div>
		<div
			v-else-if="entry.status !== 'running'"
			class="flex flex-col gap-2 text-sm text-muted-foreground"
		>
			<p>
				{{
					entry.error
						? $t('plugins.page.notRunningWithError', {
								name: entry.manifest.name,
								error: entry.error,
							})
						: $t('plugins.page.notRunning', { name: entry.manifest.name })
				}}
			</p>
			<router-link
				to="/settings/plugins"
				class="w-fit text-primary hover:underline"
			>
				{{ $t('plugins.page.openSettings') }}
			</router-link>
		</div>
		<p
			v-else-if="view?.ui && !windowsSupported"
			class="text-sm text-muted-foreground"
		>
			{{ unavailableText }}
		</p>
		<div v-else-if="view?.ui" class="flex flex-col items-start gap-3 text-sm">
			<p class="text-muted-foreground">
				{{ $t('plugins.page.ownWindow', { title: view.title }) }}
			</p>
			<Button size="sm" @click="openWindow">{{
				$t('plugins.page.openWindow')
			}}</Button>
			<p v-if="windowError" class="text-red-600 dark:text-red-400">
				{{ windowError }}
			</p>
		</div>
		<p v-else-if="error" class="text-sm text-red-600 dark:text-red-400">
			{{ error }}
		</p>
		<div v-else-if="tree" data-selectable class="flex min-w-0 flex-1 flex-col">
			<PluginView
				:key="`${pluginId}/${viewId}`"
				:node="tree"
				:plugin-id="pluginId"
			/>
		</div>
		<p v-else class="text-sm text-muted-foreground">
			{{ $t('common.states.loading') }}
		</p>
	</PageContainer>
</template>

<script lang="ts">
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PluginView from '@/components/plugins/PluginView.vue';
	import { Button } from '@/components/ui/button';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { t } from '@/i18n';
	import { pluginHost, pluginState } from '@/pluginSystem/state';
	import type { UiNode } from '@/pluginSystem/uiTree';
	import {
		pluginWindowsUnavailable,
		supportsPluginWindows,
	} from '@/utils/desktop';
	import { sanitizeDeepLinkParams } from '@/utils/desktopShortcuts';
	import { computed, defineComponent, ref, watch } from 'vue';
	import { useRoute } from 'vue-router';

	export default defineComponent({
		name: 'PluginPage',
		components: { Button, PageContainer, PluginView },
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
			const windowsSupported = supportsPluginWindows();
			const windowError = ref<string | null>(null);
			const openWindowSafely = async (props?: Record<string, string>) => {
				windowError.value = null;
				try {
					await pluginHost()?.openView(pluginId.value, viewId.value, props);
				} catch (e) {
					windowError.value = t('plugins.page.openFailed', {
						error: e instanceof Error ? e.message : String(e),
					});
				}
			};
			const openWindow = () => openWindowSafely();
			const error = ref<string | null>(null);
			let request = 0;
			let openedFor = '';
			// Which pluginId/viewId `tree` was last drawn for: switching target clears it at
			// once, so a click can never run the old plugin's command under the new plugin id.
			let treeFor = '';

			/** Query params of an in-app deep link (`tmgr://plugin/<id>/view/<id>?...`), re-sanitised here too. */
			const queryProps = computed<Record<string, string> | null>(() => {
				const keys = Object.keys(route.query);
				if (!keys.length) return null;
				const entries: [string, string][] = [];
				for (const key of keys) {
					const value = route.query[key];
					const v = Array.isArray(value) ? value[0] : value;
					if (typeof v === 'string') entries.push([key, v]);
				}
				return sanitizeDeepLinkParams(entries) ?? {};
			});

			const render = async () => {
				const current = ++request;
				const target = `${pluginId.value}/${viewId.value}`;
				if (target !== treeFor) {
					treeFor = target;
					tree.value = null;
					error.value = null;
				}
				if (entry.value?.status !== 'running') return;
				if (view.value?.ui) {
					setDocumentTitle(view.value.title);
					const key = `${pluginId.value}/${viewId.value}`;
					if (openedFor !== key) {
						openedFor = key;
						if (windowsSupported) {
							void openWindowSafely(queryProps.value ?? undefined);
						}
					}
					return;
				}
				setDocumentTitle(view.value?.title ?? entry.value.manifest.name);
				try {
					const next = await pluginHost()?.renderPage(
						pluginId.value,
						viewId.value,
						queryProps.value,
					);
					if (current !== request) return;
					tree.value = next ?? null;
					error.value = next ? null : t('plugins.page.nothingToShow');
				} catch (e) {
					if (current !== request) return;
					error.value = t('plugins.page.drawFailed', {
						error: e instanceof Error ? e.message : String(e),
					});
				}
			};

			watch(
				() => [
					pluginId.value,
					viewId.value,
					entry.value?.status,
					pluginState.revision,
					pluginState.revisions[pluginId.value],
					JSON.stringify(route.query),
				],
				() => void render(),
				{ immediate: true },
			);

			return {
				pluginId,
				viewId,
				entry,
				tree,
				error,
				view,
				openWindow,
				windowsSupported,
				windowError,
				unavailableText: computed(() => pluginWindowsUnavailable()),
			};
		},
	});
</script>
