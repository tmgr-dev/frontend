<template>
	<PageContainer width="narrow" data-testid="workspace-memory">
		<PageHeader
			title="Workspace memory"
			subtitle="Facts the AI assistant remembers about this workspace. Every member can read them."
		/>

		<div
			v-if="isLocalOnly"
			class="rounded-lg border border-line bg-surface-sunken p-4 text-sm text-ink-subtle"
		>
			Memory is available only in cloud workspaces.
		</div>
		<p v-else-if="!loaded && !failed" class="text-sm text-ink-subtle">
			Loading…
		</p>
		<div v-else-if="failed" class="flex items-center gap-3">
			<p class="text-sm text-destructive">Failed to load memory</p>
			<Button variant="outline" size="sm" @click="refreshAll">Retry</Button>
		</div>
		<div
			v-else-if="!memory.pages_enabled"
			class="rounded-lg border border-line bg-surface-sunken p-4 text-sm text-ink-subtle"
			data-testid="memory-pages-off"
		>
			Memory is kept in Pages. Turn on Pages for this workspace to use it.
			<router-link to="/settings/modules" class="ml-1 underline">
				Open modules
			</router-link>
		</div>

		<template v-else>
			<section v-if="cards.length" class="mb-8" data-testid="memory-proposals">
				<h2 class="mb-3 text-base font-semibold text-ink">
					Waiting to be added
				</h2>
				<ul class="space-y-3">
					<li
						v-for="card in cards"
						:key="card.id"
						class="rounded-lg border border-line bg-surface p-4"
						data-testid="memory-proposal"
					>
						<div class="mb-1 text-xs font-medium text-ink-subtle uppercase">
							{{ topicTitle(card.topic) }}
						</div>
						<template v-if="editingId === card.id">
							<Textarea
								v-model="editText"
								:maxlength="MAX_FACT_LENGTH"
								rows="3"
								aria-label="Edit fact"
							/>
							<div class="mt-2 flex gap-2">
								<Button
									size="sm"
									:disabled="busy || !editText.trim()"
									@click="saveEdit(card)"
								>
									Save
								</Button>
								<Button variant="outline" size="sm" @click="editingId = null">
									Cancel
								</Button>
							</div>
						</template>
						<template v-else>
							<p class="text-sm [overflow-wrap:anywhere] text-ink">
								{{ card.fact }}
							</p>
							<div class="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1">
								<span class="text-xs text-ink-subtle">
									<template v-if="card.task_id">
										Source:
										<router-link
											:to="`/${workspaceCode}/tasks/${card.task_id}`"
											class="underline"
										>
											{{ card.source || `Task #${card.task_id}` }}
										</router-link>
									</template>
									<template v-else-if="card.source">
										Source: {{ card.source }}
									</template>
								</span>
								<span
									v-if="alreadyAdded[card.id]"
									class="text-xs font-medium text-ink"
								>
									Already added
								</span>
								<span
									v-else-if="card.auto_add_at"
									class="text-xs font-medium text-ink"
									data-testid="memory-countdown"
								>
									{{ countdownLabel(card) }}
								</span>
								<span v-else class="text-xs font-medium text-ink">
									Needs your approval
								</span>
							</div>
							<div class="mt-3 flex flex-wrap gap-2">
								<Button
									v-if="alreadyAdded[card.id]"
									variant="outline"
									size="sm"
									:disabled="busy"
									@click="deleteAdded(card)"
								>
									Delete
								</Button>
								<template v-else>
									<Button
										variant="outline"
										size="sm"
										:disabled="busy"
										@click="reject(card)"
									>
										Reject
									</Button>
									<Button
										variant="outline"
										size="sm"
										:disabled="busy"
										@click="startEdit(card)"
									>
										Edit
									</Button>
									<Button size="sm" :disabled="busy" @click="acceptNow(card)">
										Add now
									</Button>
								</template>
							</div>
						</template>
					</li>
				</ul>
			</section>

			<form
				v-if="memory.can_edit"
				class="mb-8 flex flex-col gap-2 sm:flex-row"
				data-testid="memory-add-form"
				@submit.prevent="addEntry"
			>
				<Select v-model="newTopic">
					<SelectTrigger class="sm:w-56" aria-label="Topic">
						<SelectValue />
					</SelectTrigger>
					<SelectContent>
						<SelectItem
							v-for="topic in memory.topics"
							:key="topic.key"
							:value="topic.key"
						>
							{{ topic.title }}
						</SelectItem>
					</SelectContent>
				</Select>
				<Input
					v-model="newFact"
					:maxlength="MAX_FACT_LENGTH"
					placeholder="Add a fact"
					aria-label="Fact"
				/>
				<Button type="submit" :disabled="busy || !newFact.trim()">Add</Button>
			</form>

			<section
				v-for="topic in memory.topics"
				:key="topic.key"
				class="mb-8"
				:data-testid="`memory-topic-${topic.key}`"
			>
				<h2 class="text-base font-semibold text-ink">{{ topic.title }}</h2>
				<p class="mb-3 text-sm text-ink-subtle">{{ topic.description }}</p>
				<p v-if="!topic.entries.length" class="text-sm text-ink-subtle">
					Nothing saved yet.
				</p>
				<ul v-else class="divide-y divide-line rounded-lg border border-line">
					<li
						v-for="entry in newestFirst(topic.entries)"
						:key="entry.id"
						class="flex items-start gap-3 px-4 py-3"
						data-testid="memory-entry"
					>
						<div class="min-w-0 flex-1">
							<p class="text-sm [overflow-wrap:anywhere] text-ink">
								{{ entry.fact }}
								<span
									v-if="entry.stale"
									class="ml-1 rounded-pill bg-surface-sunken px-2 py-0.5 text-xs text-ink-subtle"
								>
									stale
								</span>
							</p>
							<p class="mt-1 text-xs text-ink-subtle">
								{{ entry.date
								}}<template v-if="entry.source"> · {{ entry.source }}</template>
							</p>
						</div>
						<Button
							v-if="memory.can_edit"
							variant="outline"
							size="sm"
							:disabled="busy"
							@click="removeEntry(entry)"
						>
							Delete
						</Button>
					</li>
				</ul>
			</section>
		</template>
	</PageContainer>
</template>

<script lang="ts">
	import {
		acceptMemoryProposal,
		addMemoryEntry,
		deleteMemoryEntry,
		getMemoryProposals,
		getWorkspaceMemory,
		memoryErrorMessage,
		rejectMemoryProposal,
		restoreMemoryEntry,
		updateMemoryProposal,
		type MemoryEntry,
		type MemoryProposal,
		type MemoryTopicKey,
		type WorkspaceMemory,
	} from '@/actions/tmgr/memory';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue,
	} from '@/components/ui/select';
	import { Textarea } from '@/components/ui/textarea';
	import { ToastAction, useToast } from '@/components/ui/toast';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import store from '@/store';
	import {
		formatCountdown,
		MAX_FACT_LENGTH,
		newestFirst,
		secondsLeft,
		UNDO_WINDOW_MS,
	} from '@/utils/memory';
	import {
		computed,
		defineComponent,
		h,
		onBeforeUnmount,
		onMounted,
		reactive,
		ref,
		watch,
	} from 'vue';
	import { useRoute } from 'vue-router';

	const POLL_MS = 30000;

	export default defineComponent({
		name: 'WorkspaceMemory',
		components: {
			Button,
			Input,
			PageContainer,
			PageHeader,
			Select,
			SelectContent,
			SelectItem,
			SelectTrigger,
			SelectValue,
			Textarea,
		},
		setup() {
			const route = useRoute();
			const { toast } = useToast();

			const workspaceCode = computed(() => String(route.params.workspace_code));
			const workspaceId = computed(() =>
				Number(store.getters.currentWorkspaceId),
			);
			const isLocalOnly = computed(() => store.getters.isLocalOnly === true);

			const memory = ref<WorkspaceMemory>({
				pages_enabled: true,
				can_edit: false,
				topics: [],
			});
			const proposals = ref<MemoryProposal[]>([]);
			const alreadyAdded = reactive<Record<number, string>>({});
			const addedCards = ref<MemoryProposal[]>([]);
			const loaded = ref(false);
			const failed = ref(false);
			const busy = ref(false);
			const now = ref(Date.now());
			const expired = new Set<number>();

			const editingId = ref<number | null>(null);
			const editText = ref('');
			const newTopic = ref<MemoryTopicKey>('decisions');
			const newFact = ref('');

			const cards = computed(() => {
				const ids = new Set(proposals.value.map((p) => p.id));
				return [
					...proposals.value,
					...addedCards.value.filter((card) => !ids.has(card.id)),
				];
			});

			const topicTitle = (key: MemoryTopicKey) =>
				memory.value.topics.find((t) => t.key === key)?.title ?? key;

			const countdownLabel = (card: MemoryProposal) => {
				const left = secondsLeft(card.auto_add_at, now.value);
				if (left === null) return '';
				return left > 0 ? `Added in ${formatCountdown(left)}` : 'Adding…';
			};

			const notify = (title: string, destructive = false) =>
				toast({ title, variant: destructive ? 'destructive' : 'default' });

			const fail = (error: any, fallback: string) =>
				notify(memoryErrorMessage(error, fallback), true);

			const refreshProposals = async () => {
				if (!workspaceId.value || !memory.value.pages_enabled) return;
				try {
					proposals.value = await getMemoryProposals(workspaceId.value);
					const live = new Set(proposals.value.map((p) => p.id));
					expired.forEach((id) => live.has(id) || expired.delete(id));
				} catch {
					// the next poll retries
				}
			};

			const refreshMemory = async () => {
				if (!workspaceId.value) return;
				try {
					memory.value = await getWorkspaceMemory(workspaceId.value);
					failed.value = false;
					if (!memory.value.topics.some((t) => t.key === newTopic.value)) {
						newTopic.value = memory.value.topics[0]?.key ?? newTopic.value;
					}
				} catch {
					if (!loaded.value) failed.value = true;
				}
				loaded.value = true;
			};

			const refreshAll = async () => {
				await refreshMemory();
				await refreshProposals();
			};

			const run = async (action: () => Promise<void>) => {
				busy.value = true;
				try {
					await action();
				} finally {
					busy.value = false;
				}
			};

			const acceptNow = (card: MemoryProposal) =>
				run(async () => {
					try {
						await acceptMemoryProposal(workspaceId.value, card.id);
					} catch (error) {
						fail(error, 'Could not add this fact');
					}
					await refreshAll();
				});

			const reject = (card: MemoryProposal) =>
				run(async () => {
					try {
						const result = await rejectMemoryProposal(
							workspaceId.value,
							card.id,
						);
						if (result.outcome === 'already_added') {
							alreadyAdded[card.id] = result.proposal.entry_id ?? '';
							addedCards.value = [...addedCards.value, card];
						}
					} catch (error) {
						fail(error, 'Could not reject this fact');
					}
					await refreshAll();
				});

			const deleteAdded = (card: MemoryProposal) =>
				run(async () => {
					try {
						await deleteMemoryEntry(workspaceId.value, alreadyAdded[card.id]);
					} catch (error: any) {
						if (error?.response?.status === 404) {
							notify('Already removed');
						} else {
							fail(error, 'Could not delete this entry');
							return;
						}
					}
					delete alreadyAdded[card.id];
					addedCards.value = addedCards.value.filter((c) => c.id !== card.id);
					await refreshAll();
				});

			const startEdit = (card: MemoryProposal) => {
				editingId.value = card.id;
				editText.value = card.fact;
			};

			const saveEdit = (card: MemoryProposal) =>
				run(async () => {
					try {
						await updateMemoryProposal(workspaceId.value, card.id, {
							fact: editText.value.trim(),
						});
						expired.delete(card.id);
						editingId.value = null;
					} catch (error) {
						fail(error, 'Could not save the change');
					}
					await refreshProposals();
				});

			const addEntry = () =>
				run(async () => {
					try {
						await addMemoryEntry(
							workspaceId.value,
							newTopic.value,
							newFact.value.trim(),
						);
						newFact.value = '';
					} catch (error: any) {
						const status = error?.response?.status;
						if (status === 409) notify('Already in memory', true);
						else if (status === 422) fail(error, 'This fact cannot be added');
						else fail(error, 'Could not add this fact');
					}
					await refreshMemory();
				});

			const undoDelete = async (entry: MemoryEntry) => {
				try {
					await restoreMemoryEntry(workspaceId.value, entry.id);
				} catch (error) {
					fail(error, 'Could not restore this entry');
				}
				await refreshMemory();
			};

			const removeEntry = (entry: MemoryEntry) =>
				run(async () => {
					try {
						await deleteMemoryEntry(workspaceId.value, entry.id);
						toast({
							title: 'Deleted',
							duration: UNDO_WINDOW_MS,
							action: h(
								ToastAction,
								{ altText: 'Undo', onClick: () => undoDelete(entry) },
								() => 'Undo',
							),
						});
					} catch (error: any) {
						if (error?.response?.status === 404) notify('Already removed');
						else fail(error, 'Could not delete this entry');
					}
					await refreshMemory();
				});

			let clock: ReturnType<typeof setInterval> | undefined;
			let poll: ReturnType<typeof setInterval> | undefined;

			const tick = () => {
				now.value = Date.now();
				let due = false;
				for (const card of proposals.value) {
					if (
						secondsLeft(card.auto_add_at, now.value) === 0 &&
						!expired.has(card.id)
					) {
						expired.add(card.id);
						due = true;
					}
				}
				if (due) void refreshAll();
			};

			const onVisible = () => {
				if (document.visibilityState === 'visible') void refreshProposals();
			};

			onMounted(() => {
				setDocumentTitle('Workspace memory');
				if (isLocalOnly.value) return;
				void refreshAll();
				clock = setInterval(tick, 1000);
				poll = setInterval(onVisible, POLL_MS);
				document.addEventListener('visibilitychange', onVisible);
			});

			onBeforeUnmount(() => {
				clearInterval(clock);
				clearInterval(poll);
				document.removeEventListener('visibilitychange', onVisible);
			});

			watch(workspaceId, () => {
				if (isLocalOnly.value) return;
				loaded.value = false;
				proposals.value = [];
				addedCards.value = [];
				void refreshAll();
			});

			return {
				MAX_FACT_LENGTH,
				acceptNow,
				addEntry,
				alreadyAdded,
				busy,
				cards,
				countdownLabel,
				deleteAdded,
				editText,
				editingId,
				failed,
				isLocalOnly,
				loaded,
				memory,
				newFact,
				newTopic,
				newestFirst,
				refreshAll,
				reject,
				removeEntry,
				saveEdit,
				startEdit,
				topicTitle,
				workspaceCode,
			};
		},
	});
</script>
