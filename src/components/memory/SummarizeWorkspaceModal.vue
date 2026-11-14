<template>
	<Modal
		modal-class="w-[520px] p-6"
		@close="$emit('close')"
		@closingModal="$emit('close')"
	>
		<template #modal-body>
			<div class="text-ink" data-testid="summarize-modal">
				<div class="flex items-center justify-between">
					<div class="text-xl font-bold">
						{{ $t('workspace.workspaceMemory.summarize.title') }}
					</div>
					<a
						class="text-ink-subtle hover:text-ink"
						href="#close"
						:aria-label="$t('workspace.workspaceMemory.summarize.close')"
						@click.prevent="$emit('close')"
					>
						<span class="material-icons">close</span>
					</a>
				</div>

				<template v-if="phase === 'form'">
					<div class="mt-4 flex gap-2">
						<Button
							:variant="mode === 'sources' ? 'default' : 'outline'"
							size="sm"
							@click="mode = 'sources'"
						>
							{{ $t('workspace.workspaceMemory.summarize.tabSources') }}
						</Button>
						<Button
							:variant="mode === 'extend' ? 'default' : 'outline'"
							size="sm"
							@click="mode = 'extend'"
						>
							{{ $t('workspace.workspaceMemory.summarize.tabExtend') }}
						</Button>
					</div>

					<div v-if="mode === 'extend'" class="mt-4">
						<div class="mb-1 text-sm font-medium">
							{{ $t('workspace.workspaceMemory.topic') }}
						</div>
						<div class="flex flex-wrap gap-2">
							<Button
								v-for="item in topics"
								:key="item.key"
								:variant="topic === item.key ? 'default' : 'outline'"
								size="sm"
								@click="topic = item.key"
							>
								{{ item.title }}
							</Button>
						</div>
					</div>

					<div class="mt-4">
						<div class="mb-1 text-sm font-medium">
							{{ $t('workspace.workspaceMemory.summarize.period') }}
						</div>
						<div class="flex flex-wrap items-center gap-2">
							<Button
								v-for="days in PERIODS"
								:key="days"
								:variant="period === days ? 'default' : 'outline'"
								size="sm"
								@click="period = days"
							>
								{{
									$t('workspace.workspaceMemory.summarize.lastDays', {
										n: days,
									})
								}}
							</Button>
							<Button
								:variant="period === 'custom' ? 'default' : 'outline'"
								size="sm"
								@click="period = 'custom'"
							>
								{{ $t('workspace.workspaceMemory.summarize.custom') }}
							</Button>
						</div>
						<div
							v-if="period === 'custom'"
							class="mt-2 flex items-center gap-2"
						>
							<input
								v-model="from"
								type="date"
								class="rounded-md border border-line-strong bg-surface px-2 py-1 text-sm text-ink"
								:aria-label="$t('workspace.workspaceMemory.summarize.from')"
							/>
							<input
								v-model="to"
								type="date"
								class="rounded-md border border-line-strong bg-surface px-2 py-1 text-sm text-ink"
								:aria-label="$t('workspace.workspaceMemory.summarize.to')"
							/>
						</div>
					</div>

					<div v-if="categories.length" class="mt-4">
						<div class="mb-1 text-sm font-medium">
							{{ $t('workspace.workspaceMemory.summarize.categories') }}
						</div>
						<div class="space-y-1 max-h-28 overflow-y-auto">
							<label
								v-for="category in categories"
								:key="category.id"
								class="flex items-center gap-2 text-sm"
							>
								<input
									v-model="categoryIds"
									type="checkbox"
									:value="category.id"
								/>
								{{ category.title }}
							</label>
						</div>
						<p class="mt-1 text-xs text-ink-subtle">
							{{ $t('workspace.workspaceMemory.summarize.allCategories') }}
						</p>
					</div>

					<div class="mt-4">
						<div class="mb-1 text-sm font-medium">
							{{ $t('workspace.workspaceMemory.summarize.sources') }}
						</div>
						<label class="flex items-center gap-2 text-sm">
							<input
								type="checkbox"
								checked
								disabled
								data-testid="summarize-tasks"
							/>
							{{ $t('workspace.workspaceMemory.summarize.tasks') }}
						</label>
						<label class="flex items-center gap-2 text-sm">
							<input
								type="checkbox"
								checked
								disabled
								data-testid="summarize-comments"
							/>
							{{ $t('workspace.workspaceMemory.summarize.comments') }}
						</label>
						<label class="flex items-center gap-2 text-sm">
							<input
								v-model="myChats"
								type="checkbox"
								data-testid="summarize-my-chats"
							/>
							{{ $t('workspace.workspaceMemory.summarize.myChats') }}
						</label>
						<p class="ml-6 text-xs text-ink-subtle">
							{{ $t('workspace.workspaceMemory.summarize.myChatsHint') }}
						</p>
					</div>

					<p
						v-if="previewLine"
						class="mt-4 text-sm"
						data-testid="summarize-preview"
					>
						{{ previewLine }}
					</p>
					<p v-else-if="previewFailed" class="mt-4 text-sm text-destructive">
						{{ $t('workspace.workspaceMemory.summarize.previewFailed') }}
					</p>
					<p v-if="blockedReason" class="mt-1 text-sm text-destructive">
						{{ blockedReason }}
					</p>
					<p v-if="errorText" class="mt-1 text-sm text-destructive">
						{{ errorText }}
					</p>

					<div class="mt-4 flex justify-end">
						<Button
							:disabled="!canRun"
							data-testid="summarize-run"
							@click="start"
						>
							{{ $t('workspace.workspaceMemory.summarize.run') }}
						</Button>
					</div>
				</template>

				<template v-else-if="phase === 'running'">
					<p
						v-if="alreadyRunning"
						class="mt-4 text-sm font-medium"
						data-testid="summarize-already-running"
					>
						{{ $t('workspace.workspaceMemory.summarize.alreadyRunning') }}
					</p>
					<p class="mt-4 text-sm" data-testid="summarize-progress">
						{{ stageLabel }}
					</p>
					<div class="mt-2 h-2 overflow-hidden rounded-pill bg-surface-sunken">
						<div
							class="h-full bg-ink transition-all"
							:style="{ width: `${progressPercent}%` }"
						/>
					</div>
					<p v-if="errorText" class="mt-2 text-sm text-destructive">
						{{ errorText }}
					</p>
					<div class="mt-4 flex justify-end gap-2">
						<Button variant="outline" @click="$emit('close')">
							{{ $t('workspace.workspaceMemory.summarize.close') }}
						</Button>
						<Button
							variant="outline"
							:disabled="cancelling"
							data-testid="summarize-cancel"
							@click="cancel"
						>
							{{ $t('workspace.workspaceMemory.cancel') }}
						</Button>
					</div>
				</template>

				<template v-else>
					<p class="mt-4 text-sm" data-testid="summarize-finished">
						{{ finishedLabel }}
					</p>
					<p v-if="finishedNote" class="mt-1 text-xs text-ink-subtle">
						{{ finishedNote }}
					</p>
					<div class="mt-4 flex justify-end">
						<Button @click="$emit('close')">
							{{ $t('workspace.workspaceMemory.summarize.close') }}
						</Button>
					</div>
				</template>
			</div>
		</template>
	</Modal>
</template>

<script lang="ts">
	import { getCategoriesOfWorkspace } from '@/actions/tmgr/categories';
	import {
		cancelSummarizeRun,
		getCurrentSummarizeRun,
		memoryErrorMessage,
		previewSummarize,
		startSummarize,
		type MemorySummarizeDoneEvent,
		type SummarizeParams,
		type SummarizePreview,
		type SummarizeRun,
		type SummarizeStatus,
	} from '@/actions/tmgr/memory';
	import Modal from '@/components/Modal.vue';
	import { Button } from '@/components/ui/button';
	import { usePusher } from '@/composable/usePusher';
	import { t } from '@/i18n';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
		type PropType,
	} from 'vue';
	import { useStore } from 'vuex';

	const POLL_MS = 5000;
	const PERIODS = [7, 30, 90];

	const isActive = (status: SummarizeStatus) =>
		status === 'queued' || status === 'running';

	const compact = (n: number) =>
		n >= 1000 ? `${Math.round(n / 1000)}k` : String(n);

	export default defineComponent({
		name: 'SummarizeWorkspaceModal',
		components: { Button, Modal },
		props: {
			workspaceId: { type: Number, required: true },
			topics: {
				type: Array as PropType<{ key: string; title: string }[]>,
				default: () => [],
			},
		},
		emits: ['close', 'done'],
		setup(props, { emit }) {
			const store = useStore();

			const mode = ref<'sources' | 'extend'>('sources');
			const period = ref<number | 'custom'>(30);
			const from = ref('');
			const to = ref('');
			const categoryIds = ref<number[]>([]);
			const myChats = ref(false);
			const topic = ref('');
			const categories = ref<{ id: number; title: string }[]>([]);

			const preview = ref<SummarizePreview | null>(null);
			const previewFailed = ref(false);

			const phase = ref<'form' | 'running' | 'finished'>('form');
			const run = ref<SummarizeRun | null>(null);
			const alreadyRunning = ref(false);
			const starting = ref(false);
			const cancelling = ref(false);
			const errorText = ref('');
			const finished = ref<{
				status: SummarizeStatus;
				cards: number | null;
				skipped: number | null;
				partial: boolean;
			} | null>(null);

			const customReady = computed(
				() => period.value !== 'custom' || (!!from.value && !!to.value),
			);

			const params = computed<SummarizeParams>(() => ({
				mode: mode.value,
				my_chats: myChats.value,
				...(mode.value === 'extend' ? { topic: topic.value } : {}),
				...(period.value === 'custom'
					? { from: from.value, to: to.value }
					: { days: period.value }),
				...(categoryIds.value.length
					? { category_ids: [...categoryIds.value] }
					: {}),
			}));

			const sourceCount = computed(() =>
				preview.value
					? preview.value.tasks + preview.value.comments + preview.value.chats
					: 0,
			);

			const overLimit = computed(
				() =>
					!!preview.value &&
					preview.value.estimated_tokens > preview.value.left_today,
			);

			const noSources = computed(
				() => !!preview.value && mode.value === 'sources' && !sourceCount.value,
			);

			const previewLine = computed(() => {
				const p = preview.value;
				if (!p) return '';
				const parts = [
					t('workspace.workspaceMemory.summarize.previewTasks', p.tasks),
					t('workspace.workspaceMemory.summarize.previewComments', p.comments),
				];
				if (myChats.value) {
					parts.push(
						t('workspace.workspaceMemory.summarize.previewChats', p.chats),
					);
				}
				parts.push(
					t('workspace.workspaceMemory.summarize.previewTail', {
						tokens: compact(p.estimated_tokens),
						left: compact(p.left_today),
					}),
				);
				return parts.join(', ');
			});

			const blockedReason = computed(() => {
				if (overLimit.value)
					return t('workspace.workspaceMemory.summarize.overLimit');
				if (noSources.value)
					return t('workspace.workspaceMemory.summarize.noSources');
				return '';
			});

			const canRun = computed(
				() =>
					phase.value === 'form' &&
					!starting.value &&
					!!preview.value &&
					!overLimit.value &&
					!noSources.value &&
					(mode.value === 'sources' || !!topic.value) &&
					customReady.value,
			);

			let previewSeq = 0;
			const loadPreview = async () => {
				const seq = ++previewSeq;
				preview.value = null;
				previewFailed.value = false;
				if (!customReady.value) return;
				if (mode.value === 'extend' && !topic.value) return;
				try {
					const result = await previewSummarize(
						props.workspaceId,
						params.value,
					);
					if (seq === previewSeq) preview.value = result;
				} catch {
					if (seq === previewSeq) previewFailed.value = true;
				}
			};

			let poll: ReturnType<typeof setInterval> | undefined;
			let polling = false;
			let channelName: string | null = null;
			let subscriptionId = '';

			const stopPolling = () => {
				clearInterval(poll);
				poll = undefined;
			};

			const finish = (
				status: SummarizeStatus,
				event?: MemorySummarizeDoneEvent,
			) => {
				stopPolling();
				finished.value = {
					status,
					cards: event?.cards ?? null,
					skipped: event?.skipped ?? run.value?.sources_skipped ?? null,
					partial: event?.partial ?? false,
				};
				phase.value = 'finished';
				emit('done');
			};

			const adopt = (next: SummarizeRun) => {
				run.value = next;
				if (!isActive(next.status)) finish(next.status);
			};

			const pollRun = async () => {
				if (polling || !run.value) return;
				polling = true;
				try {
					const current = await getCurrentSummarizeRun(props.workspaceId);
					if (!current || current.id !== run.value?.id) {
						if (phase.value === 'running') finish('done');
					} else if (phase.value === 'running') {
						adopt(current);
					}
				} catch {
				} finally {
					polling = false;
				}
			};

			const beginRunning = (next: SummarizeRun) => {
				run.value = next;
				phase.value = 'running';
				stopPolling();
				poll = setInterval(pollRun, POLL_MS);
			};

			const adoptCurrent = async (): Promise<boolean> => {
				try {
					const current = await getCurrentSummarizeRun(props.workspaceId);
					if (current && isActive(current.status)) {
						alreadyRunning.value = true;
						beginRunning(current);
						return true;
					}
				} catch {}
				return false;
			};

			const start = async () => {
				if (!canRun.value) return;
				starting.value = true;
				errorText.value = '';
				try {
					beginRunning(await startSummarize(props.workspaceId, params.value));
				} catch (error: any) {
					const data = error?.response?.data;
					if (
						error?.response?.status === 409 &&
						data?.error === 'already_running'
					) {
						alreadyRunning.value = true;
						if (!(await adoptCurrent())) {
							errorText.value = memoryErrorMessage(error, '');
						}
					} else {
						errorText.value = memoryErrorMessage(
							error,
							t('workspace.workspaceMemory.summarize.startFailed'),
						);
					}
				} finally {
					starting.value = false;
				}
			};

			const cancel = async () => {
				if (!run.value) return;
				cancelling.value = true;
				errorText.value = '';
				try {
					adopt(await cancelSummarizeRun(props.workspaceId, run.value.id));
				} catch (error) {
					errorText.value = memoryErrorMessage(
						error,
						t('workspace.workspaceMemory.summarize.cancelFailed'),
					);
				} finally {
					cancelling.value = false;
				}
			};

			const stageLabel = computed(() => {
				const current = run.value;
				const prefix = 'workspace.workspaceMemory.summarize.';
				if (!current || current.status === 'queued' || !current.stage)
					return t(`${prefix}stageQueued`);
				if (current.stage === 'reading')
					return t(`${prefix}stageReading`, {
						done: current.progress_done,
						total: current.progress_total,
					});
				if (current.stage === 'extracting')
					return t(`${prefix}stageExtracting`);
				if (current.stage === 'comparing') return t(`${prefix}stageComparing`);
				return t(`${prefix}stageWorking`);
			});

			const progressPercent = computed(() => {
				const current = run.value;
				if (!current || !current.progress_total) return 0;
				return Math.min(
					100,
					Math.round((current.progress_done / current.progress_total) * 100),
				);
			});

			const finishedLabel = computed(() => {
				const info = finished.value;
				const prefix = 'workspace.workspaceMemory.summarize.';
				if (!info) return '';
				if (info.status === 'failed') return t(`${prefix}failed`);
				if (info.status === 'cancelled') return t(`${prefix}cancelled`);
				return info.cards === null
					? t(`${prefix}doneNoCount`)
					: t(`${prefix}done`, info.cards);
			});

			const finishedNote = computed(() => {
				const info = finished.value;
				const prefix = 'workspace.workspaceMemory.summarize.';
				const notes: string[] = [];
				if (info?.partial) notes.push(t(`${prefix}partial`));
				if (info?.skipped) notes.push(t(`${prefix}skipped`, info.skipped));
				return notes.join(' ');
			});

			watch([mode, period, from, to, topic, myChats, categoryIds], () => {
				if (phase.value === 'form') void loadPreview();
			});

			onMounted(async () => {
				const userId = store.state.user?.id;
				if (userId) {
					channelName = `App.User.${userId}`;
					subscriptionId = usePusher().subscribeToUser(userId, {
						onMemorySummarizeProgress: (event) => {
							if (!run.value || event.run_id !== run.value.id) return;
							run.value = {
								...run.value,
								status: 'running',
								stage: event.stage,
								progress_done: event.done,
								progress_total: event.total,
							};
						},
						onMemorySummarizeDone: (event) => {
							if (!run.value || event.run_id !== run.value.id) return;
							if (phase.value === 'running') finish(event.status, event);
						},
					});
				}
				getCategoriesOfWorkspace(props.workspaceId)
					.then((list) => {
						categories.value = list.map((c) => ({ id: c.id, title: c.title }));
					})
					.catch(() => undefined);
				if (!(await adoptCurrent())) await loadPreview();
			});

			onBeforeUnmount(() => {
				stopPolling();
				if (channelName && subscriptionId) {
					usePusher().unsubscribeHandler(channelName, subscriptionId);
				}
			});

			return {
				PERIODS,
				alreadyRunning,
				blockedReason,
				cancel,
				cancelling,
				canRun,
				categories,
				categoryIds,
				errorText,
				finishedLabel,
				finishedNote,
				from,
				mode,
				myChats,
				period,
				phase,
				previewFailed,
				previewLine,
				progressPercent,
				stageLabel,
				start,
				to,
				topic,
			};
		},
	});
</script>
