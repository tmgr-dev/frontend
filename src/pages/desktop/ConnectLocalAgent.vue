<template>
	<div class="min-h-screen bg-surface-base px-4 py-8 text-ink">
		<main
			class="mx-auto max-w-2xl rounded-card border border-line bg-surface p-5 sm:p-8"
			data-testid="connect-local-agent"
		>
			<h1 class="text-xl font-semibold text-ink">
				{{ $t('desktop.connectAgent.title') }}
			</h1>

			<p v-if="view === 'loading'" class="mt-3 text-sm text-ink-subtle">
				{{ $t('desktop.connectAgent.looking') }}
			</p>
			<p
				v-else-if="view === 'unavailable'"
				class="mt-3 text-sm text-ink-subtle"
				data-tour="connect-local.agents"
			>
				{{ $t('desktop.connectAgent.unavailable') }}
			</p>
			<p v-else-if="view === 'demo'" class="mt-3 text-sm text-ink-subtle">
				{{ $t('desktop.connectAgent.demo') }}
			</p>

			<template v-else>
				<p class="mt-2 text-sm text-ink-subtle">
					{{ $t('desktop.connectAgent.pick') }}
				</p>

				<p
					v-if="connect.loadError.value"
					class="mt-3 text-sm text-red-600 dark:text-red-400"
					role="alert"
				>
					{{ connect.loadError.value }}
				</p>

				<ul
					class="mt-5 flex flex-col gap-3"
					data-tour="connect-local.agents"
					:aria-label="$t('desktop.connectAgent.agents')"
				>
					<li
						v-for="row in connect.rows.value"
						:key="row.info.id"
						class="rounded-md border border-line p-3"
						:data-testid="`agent-${row.info.id}`"
					>
						<div class="flex flex-wrap items-center justify-between gap-2">
							<label class="flex items-center gap-2 text-sm font-medium">
								<input
									v-if="row.selectable"
									type="checkbox"
									:checked="!!connect.selected[row.info.id]"
									:disabled="connect.running.value"
									@change="
										connect.toggle(
											row.info.id,
											($event.target as HTMLInputElement).checked,
										)
									"
								/>
								{{ row.info.title }}
								<span
									class="rounded px-1.5 py-0.5 text-2xs font-semibold uppercase"
									:class="
										row.found
											? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300'
											: 'bg-muted text-muted-foreground dark:bg-muted dark:text-muted-foreground'
									"
								>
									{{
										row.found
											? $t('desktop.connectAgent.found')
											: $t('desktop.connectAgent.notFound')
									}}
								</span>
							</label>
							<div class="flex gap-2">
								<Button
									v-if="row.copyOnly"
									size="sm"
									variant="outline"
									:disabled="copying === row.info.id"
									@click="copy(row.info.id)"
								>
									{{
										connect.copied.value === row.info.id
											? $t('common.actions.copied')
											: $t('desktop.connectAgent.copyConfig')
									}}
								</Button>
								<Button
									v-if="
										row.info.canInstall &&
										row.found &&
										connect.detect.value?.platformSupported
									"
									size="sm"
									variant="outline"
									:disabled="
										connect.running.value ||
										!!connect.busyRemove.value ||
										connect.detect.value?.safeMode
									"
									@click="askRemove(row.info.id)"
								>
									{{ $t('common.actions.remove') }}
								</Button>
							</div>
						</div>
						<i18n-t
							v-if="row.info.id === 'cursor'"
							keypath="desktop.connectAgent.cursorHint"
							tag="p"
							class="mt-1 text-xs text-ink-subtle"
						>
							<template #path><code>~/.cursor/mcp.json</code></template>
						</i18n-t>

						<div v-if="connect.results[row.info.id]" class="mt-3 text-sm">
							<p
								:class="
									connect.results[row.info.id].status === 'ok'
										? 'text-emerald-700 dark:text-emerald-400'
										: 'text-red-600 dark:text-red-400'
								"
								role="status"
							>
								{{ connect.results[row.info.id].message }}
							</p>
							<p
								v-if="connect.results[row.info.id].note"
								class="mt-1 text-xs text-ink-subtle"
							>
								{{ connect.results[row.info.id].note }}
							</p>
							<div
								v-for="(step, i) in connect.results[row.info.id].steps"
								:key="i"
								class="mt-2"
							>
								<pre
									class="overflow-x-auto rounded bg-muted p-2 font-mono text-xs whitespace-pre-wrap"
								>
$ {{ step.command }}</pre>
								<pre
									v-if="step.output"
									class="mt-1 overflow-x-auto rounded bg-muted p-2 font-mono text-xs whitespace-pre-wrap"
									>{{ step.output }}</pre>
							</div>
						</div>
					</li>
				</ul>

				<p
					v-if="connect.copyError.value"
					class="mt-3 text-sm text-red-600 dark:text-red-400"
					role="alert"
				>
					{{ connect.copyError.value }}
				</p>

				<p
					v-if="connect.installBlock.value"
					class="mt-4 text-sm text-amber-700 dark:text-amber-400"
					data-testid="install-block"
				>
					{{ connect.installBlock.value }}
				</p>

				<section
					v-if="connect.previewed.value"
					class="mt-5 flex flex-col gap-2"
					data-testid="preview"
				>
					<div v-for="line in connect.previewLines.value" :key="line.agent">
						<p class="text-sm font-medium">
							{{ $t('desktop.connectAgent.willRun', { title: line.title }) }}
						</p>
						<pre
							class="mt-1 overflow-x-auto rounded bg-muted p-2 font-mono text-xs whitespace-pre-wrap"
							>{{ line.commands.join('\n') }}</pre>
					</div>
					<p
						v-for="note in installNotes"
						:key="note"
						class="text-xs text-ink-subtle"
					>
						{{ note }}
					</p>
				</section>

				<div class="mt-5 flex gap-2">
					<Button
						variant="outline"
						:disabled="!connect.canPreview.value || connect.running.value"
						@click="connect.preview()"
					>
						{{ $t('desktop.connectAgent.preview') }}
					</Button>
					<Button
						data-tour="connect-local.install"
						:disabled="!connect.canInstall.value"
						@click="connect.install()"
					>
						{{
							connect.running.value
								? $t('desktop.connectAgent.installing')
								: $t('desktop.connectAgent.install')
						}}
					</Button>
				</div>
			</template>
		</main>

		<AlertDialog
			:open="!!removing"
			@update:open="(value: boolean) => !value && (removing = null)"
		>
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>{{
						$t('desktop.connectAgent.removeTitle', { agent: removingTitle })
					}}</AlertDialogTitle>
					<AlertDialogDescription>
						{{ $t('desktop.connectAgent.removeDescription') }}
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel>{{
						$t('common.actions.cancel')
					}}</AlertDialogCancel>
					<AlertDialogAction @click="confirmRemove">{{
						$t('common.actions.remove')
					}}</AlertDialogAction>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	</div>
</template>

<script lang="ts">
	import {
		AlertDialog,
		AlertDialogAction,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import {
		installNotes,
		useConnectLocalAgent,
	} from '@/composable/useConnectLocalAgent';
	import {
		agentInfo,
		type ConnectAgentId,
		type RunnableAgentId,
	} from '@/local/connectAgent';
	import { LOCAL_OWNER_ID } from '@/local/personaGate';
	import { activeLocalWorkspace, listLocalWorkspaces } from '@/local/runtime';
	import type { LocalWorkspace } from '@/local/types';
	import { isDesktopApp } from '@/utils/desktop';
	import { computed, defineComponent, onMounted, ref } from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'ConnectLocalAgent',
		components: {
			AlertDialog,
			AlertDialogAction,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
			Button,
		},
		setup() {
			const store = useStore();
			const route = useRoute();
			const router = useRouter();
			const workspace = ref<LocalWorkspace | null>(null);
			const view = ref<'loading' | 'unavailable' | 'demo' | 'ready'>('loading');
			const removing = ref<RunnableAgentId | null>(null);
			const copying = ref('');

			const currentUser = () =>
				store.getters?.isLocalOnly
					? {
							id: LOCAL_OWNER_ID,
							name:
								store.state.user?.name || store.state.localProfile?.name || '',
							email: '',
						}
					: {
							id: Number(store.state.user?.id) || 0,
							name: store.state.user?.name ?? '',
							email: store.state.user?.email ?? '',
						};

			const connect = useConnectLocalAgent({
				workspace: () => workspace.value as LocalWorkspace,
				user: currentUser,
				onModulesChanged: async () => {
					const id = store.getters.currentWorkspaceId;
					if (id != null)
						await store.dispatch('featureToggles/fetchWorkspaceModules', id);
				},
			});

			onMounted(async () => {
				if (!isDesktopApp()) {
					view.value = 'unavailable';
					return;
				}
				await listLocalWorkspaces();
				const active = activeLocalWorkspace();
				if (!active) {
					view.value = 'unavailable';
					return;
				}
				workspace.value = active;
				if (active.demo) {
					view.value = 'demo';
					return;
				}
				if (route.params.workspace_code !== active.code) {
					await router.replace({
						name: 'ConnectLocalAgent',
						params: { workspace_code: active.code },
					});
				}
				view.value = 'ready';
				await connect.load();
			});

			const askRemove = (id: ConnectAgentId) => {
				removing.value = id as RunnableAgentId;
			};
			const confirmRemove = async () => {
				const agent = removing.value;
				removing.value = null;
				if (agent) await connect.remove(agent);
			};
			const copy = async (id: ConnectAgentId) => {
				copying.value = id;
				try {
					await connect.copyConfig(id);
				} finally {
					copying.value = '';
				}
			};
			const removingTitle = computed(() =>
				removing.value ? agentInfo(removing.value).title : '',
			);

			return {
				connect,
				view,
				removing,
				removingTitle,
				copying,
				askRemove,
				confirmRemove,
				copy,
				installNotes: computed(() => installNotes()),
			};
		},
	});
</script>
