<template>
	<Dialog :open="true" @update:open="onOpenChange">
		<DialogContent
			class="rounded-card border border-line bg-surface text-ink sm:max-w-[560px]"
		>
			<DialogHeader>
				<DialogTitle>{{ $t('general.daySummary.title') }}</DialogTitle>
			</DialogHeader>

			<p v-if="loading" class="text-sm text-ink-subtle">
				{{ $t('general.daySummary.collecting') }}
			</p>
			<div v-else class="flex flex-col gap-1 text-sm">
				<p>
					{{
						$t('general.daySummary.counts', {
							tasks: tasks.length,
							workspaces: workspaceCount,
						})
					}}
				</p>
				<p v-if="skipped.length" class="text-amber-600 dark:text-amber-400">
					{{ $t('general.daySummary.skipped', { names: skipped.join(', ') }) }}
				</p>
			</div>

			<p v-if="isPastDay" class="text-xs text-ink-subtle">
				{{ $t('general.daySummary.pastDayNote') }}
			</p>

			<div
				v-if="lanOrigin"
				class="flex flex-col gap-2 rounded-md border border-amber-300 bg-amber-50 p-3 text-sm dark:border-amber-700 dark:bg-amber-950/40"
			>
				<p class="text-amber-800 dark:text-amber-200">
					{{ $t('general.daySummary.lanNotice', { origin: lanOrigin }) }}
				</p>
				<div class="flex gap-2">
					<Button size="sm" @click="sendToLan">{{
						$t('general.daySummary.send')
					}}</Button>
					<Button size="sm" variant="outline" @click="cancelLan">{{
						$t('common.actions.cancel')
					}}</Button>
				</div>
			</div>

			<div v-if="summary !== null" class="flex flex-col gap-2">
				<span
					class="w-fit rounded bg-muted px-1.5 py-0.5 text-2xs font-semibold text-muted-foreground uppercase"
				>
					{{
						aiGenerated
							? $t('general.daySummary.aiGenerated')
							: $t('general.daySummary.noAi')
					}}
				</span>
				<p class="text-sm whitespace-pre-line">{{ summary }}</p>
				<RouterLink
					v-if="!aiGenerated"
					:to="{ name: 'PersonaSettings' }"
					class="w-fit text-sm text-primary hover:underline"
				>
					{{ $t('general.daySummary.setUpAi') }}
				</RouterLink>
			</div>

			<p v-if="notice" class="text-sm text-ink-subtle">{{ notice }}</p>
			<p v-if="error" class="text-sm text-red-600 dark:text-red-400">
				{{ error }}
			</p>

			<DialogFooter class="flex flex-wrap gap-2">
				<Button
					v-if="summary === null"
					:disabled="loading || summarizing"
					@click="summarize"
				>
					{{
						summarizing
							? $t('general.daySummary.summarizing')
							: $t('general.daySummary.summarize')
					}}
				</Button>
				<Button
					v-else
					variant="outline"
					:disabled="summarizing || !!lanOrigin"
					@click="summarize"
				>
					{{ $t('general.daySummary.regenerate') }}
				</Button>
				<p v-if="aiOrigin" class="basis-full text-xs text-ink-subtle">
					{{ $t('general.daySummary.sendsTo', { origin: aiOrigin }) }}
				</p>
				<Button
					variant="outline"
					:disabled="summary === null"
					@click="copySummary"
				>
					{{ $t('common.actions.copy') }}
				</Button>
				<Button
					v-if="localWorkspace && summary !== null"
					variant="outline"
					@click="savePage"
				>
					{{ $t('general.daySummary.saveAsPage') }}
				</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import { t } from '@/i18n';
	import { getWorkspaces } from '@/actions/tmgr/workspaces';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import {
		buildDayInput,
		buildMessages,
		deterministicSummary,
		sanitizeSummary,
		stripThink,
		type DayTask,
	} from '@/local/daySummary';
	import { collectDay } from '@/local/daySummaryCollect';
	import {
		llmComplete,
		llmLanConfirm,
		needsConfirmationOrigin,
	} from '@/local/llmClient';
	import { createPage } from '@/local/pages/service';
	import { LOCAL_OWNER_ID } from '@/local/personaGate';
	import { activeLocalWorkspace, localContext } from '@/local/runtime';
	import { resyncCloudIdentity } from '@/utils/cloudIdentity';
	import { computed, defineComponent, onMounted, ref } from 'vue';
	import { RouterLink } from 'vue-router';
	import { useStore } from 'vuex';

	const messageOf = (error: unknown): string =>
		error instanceof Error ? error.message : String(error);

	const originOf = (url: string): string | null => {
		try {
			const { origin } = new URL(url);
			return origin === 'null' ? null : origin;
		} catch {
			return null;
		}
	};

	export default defineComponent({
		name: 'DaySummaryModal',
		components: {
			Button,
			Dialog,
			DialogContent,
			DialogFooter,
			DialogHeader,
			DialogTitle,
			RouterLink,
		},
		props: {
			day: { type: Date, default: undefined },
		},
		emits: ['close'],
		setup(props, { emit }) {
			const store = useStore();
			const localOnly = !!store.getters?.isLocalOnly;
			const userId = () =>
				localOnly ? LOCAL_OWNER_ID : Number(store.state.user?.id) || 0;
			const localUser = () => ({
				id: userId(),
				name: store.state.user?.name || store.state.localProfile?.name || '',
				email: localOnly ? '' : (store.state.user?.email ?? ''),
			});
			const localWorkspace = activeLocalWorkspace();

			const loading = ref(true);
			const summarizing = ref(false);
			const tasks = ref<DayTask[]>([]);
			const skipped = ref<string[]>([]);
			const summary = ref<string | null>(null);
			const aiGenerated = ref(false);
			const lanOrigin = ref<string | null>(null);
			const error = ref<string | null>(null);
			const notice = ref<string | null>(null);
			const aiOrigin = ref<string | null>(null);
			const isPastDay = !!props.day;
			const workspaceCount = computed(
				() => new Set(tasks.value.map((task) => task.workspace)).size,
			);

			const cloudWorkspaces = async () => {
				if (localOnly) return [];
				try {
					return (await getWorkspaces())
						.filter((workspace) => workspace.id >= 0)
						.map((workspace) => ({ id: workspace.id, name: workspace.name }));
				} catch {
					return [];
				}
			};

			const load = async () => {
				loading.value = true;
				aiOrigin.value = await baseUrl().then(originOf, () => null);
				try {
					const result = await collectDay({
						now: props.day ?? new Date(),
						localUserId: userId(),
						cloud: await cloudWorkspaces(),
						cloudUserId: localOnly
							? null
							: Number(store.state.user?.id) || null,
					});
					tasks.value = result.tasks;
					skipped.value = result.skipped;
				} catch (caught) {
					error.value = messageOf(caught);
				} finally {
					loading.value = false;
				}
			};

			onMounted(load);

			const baseUrl = async () => {
				await resyncCloudIdentity();
				const core = await import('@tauri-apps/api/core');
				const config = await core.invoke<{ base_url: string }>(
					'llm_config_get',
					{ userId: userId() },
				);
				return config.base_url;
			};

			const fallback = () => {
				summary.value = deterministicSummary(tasks.value);
				aiGenerated.value = false;
			};

			const summarize = async () => {
				error.value = null;
				notice.value = null;
				summarizing.value = true;
				try {
					if (!tasks.value.length || !(await baseUrl())) {
						fallback();
						return;
					}
					const { system, prompt } = buildMessages(
						buildDayInput(tasks.value).text,
					);
					const answer = sanitizeSummary(
						stripThink(
							await llmComplete(userId(), {
								system,
								prompt,
								maxTokens: 800,
								temperature: 0.2,
							}),
						),
					);
					if (!answer) {
						fallback();
						return;
					}
					summary.value = answer;
					aiGenerated.value = true;
				} catch (caught) {
					const origin = needsConfirmationOrigin(caught);
					if (origin) {
						lanOrigin.value = origin;
						return;
					}
					error.value = messageOf(caught);
					fallback();
				} finally {
					summarizing.value = false;
				}
			};

			const sendToLan = async () => {
				const origin = lanOrigin.value;
				if (!origin) return;
				lanOrigin.value = null;
				try {
					await llmLanConfirm(userId(), origin, true);
				} catch (caught) {
					error.value = messageOf(caught);
					return;
				}
				await summarize();
			};

			const cancelLan = () => {
				lanOrigin.value = null;
				fallback();
			};

			const copySummary = async () => {
				if (summary.value === null) return;
				try {
					await navigator.clipboard.writeText(summary.value);
					notice.value = t('general.daySummary.copied');
				} catch (caught) {
					error.value = messageOf(caught);
				}
			};

			const savePage = async () => {
				if (!localWorkspace || summary.value === null) return;
				try {
					await createPage(await localContext(localWorkspace, localUser()), {
						title: t('general.daySummary.pageTitle', {
							date: (props.day ?? new Date()).toLocaleDateString('en-CA'),
						}),
						body: summary.value,
					});
					notice.value = t('general.daySummary.savedAsPage');
				} catch (caught) {
					error.value = messageOf(caught);
				}
			};

			const onOpenChange = (open: boolean) => {
				if (!open) emit('close');
			};

			return {
				loading,
				summarizing,
				tasks,
				skipped,
				summary,
				aiGenerated,
				lanOrigin,
				error,
				notice,
				aiOrigin,
				workspaceCount,
				isPastDay,
				localWorkspace,
				summarize,
				sendToLan,
				cancelLan,
				copySummary,
				savePage,
				onOpenChange,
			};
		},
	});
</script>
