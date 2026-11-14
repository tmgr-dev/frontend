<template>
	<PageContainer width="narrow" data-testid="persona-memory-screen">
		<PageHeader
			:title="$t('settingsPages.personaMemory.title')"
			:subtitle="$t('settingsPages.personaMemory.subtitle')"
		>
			<template #actions>
				<Button size="sm" :disabled="!personaOptions.length" @click="startAdd">
					{{ $t('settingsPages.personaMemory.add') }}
				</Button>
			</template>
		</PageHeader>

		<p class="mb-4 text-xs text-ink-subtle">
			<router-link
				:to="`/${workspaceCode}/memory`"
				class="underline"
				data-testid="assistant-memory-link"
			>
				{{ $t('settingsPages.personaMemory.assistantMemory') }}
			</router-link>
		</p>

		<div class="mb-4 flex flex-wrap items-center gap-2">
			<select
				v-model="personaFilter"
				:class="fieldClass"
				:aria-label="$t('settingsPages.personaMemory.persona')"
				data-testid="memory-persona-filter"
			>
				<option value="">
					{{ $t('settingsPages.personaMemory.allPersonas') }}
				</option>
				<option v-for="p in personaOptions" :key="p.uuid" :value="p.uuid">
					{{ p.name }}
				</option>
			</select>
			<select
				v-model="topicFilter"
				:class="fieldClass"
				:aria-label="$t('settingsPages.personaMemory.topic')"
				data-testid="memory-topic-filter"
			>
				<option value="">
					{{ $t('settingsPages.personaMemory.allTopics') }}
				</option>
				<option v-for="topic in MEMORY_TOPICS" :key="topic" :value="topic">
					{{ topicTitle(topic) }}
				</option>
			</select>
			<input
				v-model="query"
				type="search"
				:class="[fieldClass, 'min-w-0 flex-1']"
				:placeholder="$t('settingsPages.personaMemory.search')"
				:aria-label="$t('settingsPages.personaMemory.search')"
				data-testid="memory-search"
			/>
			<label class="flex items-center gap-2 text-sm">
				<Switch :checked="includeJournal" @update:checked="setJournal" />
				{{ $t('settingsPages.personaMemory.includeJournal') }}
			</label>
		</div>

		<form
			v-if="adding"
			class="mb-4 flex flex-col gap-2 rounded-lg border border-line bg-surface p-3"
			data-testid="memory-add-form"
			@submit.prevent="submitAdd"
		>
			<h2 class="text-sm font-semibold text-ink">
				{{ $t('settingsPages.personaMemory.addTitle') }}
			</h2>
			<div class="flex flex-wrap gap-2">
				<select
					v-model="draft.persona"
					:class="fieldClass"
					:aria-label="$t('settingsPages.personaMemory.persona')"
				>
					<option v-for="p in personaOptions" :key="p.uuid" :value="p.uuid">
						{{ p.name }}
					</option>
				</select>
				<select
					v-model="draft.topic"
					:class="fieldClass"
					:aria-label="$t('settingsPages.personaMemory.topic')"
				>
					<option v-for="topic in MEMORY_TOPICS" :key="topic" :value="topic">
						{{ topicTitle(topic) }}
					</option>
				</select>
				<label class="flex items-center gap-2 text-xs text-ink-subtle">
					{{ $t('settingsPages.personaMemory.checkByOptional') }}
					<input v-model="draft.checkBy" type="date" :class="fieldClass" />
				</label>
			</div>
			<Textarea
				v-model="draft.body"
				rows="3"
				:aria-label="$t('settingsPages.personaMemory.text')"
				:placeholder="$t('settingsPages.personaMemory.text')"
			/>
			<div class="flex gap-2">
				<Button type="submit" size="sm" :disabled="busy || !draft.body.trim()">
					{{ $t('settingsPages.personaMemory.save') }}
				</Button>
				<Button
					type="button"
					variant="outline"
					size="sm"
					@click="adding = false"
				>
					{{ $t('settingsPages.personaMemory.cancel') }}
				</Button>
			</div>
		</form>

		<p v-if="loading" class="text-sm text-ink-subtle">
			{{ $t('settingsPages.personaMemory.loading') }}
		</p>
		<div v-else-if="failed" class="flex items-center gap-3">
			<p class="text-sm text-destructive">
				{{ $t('settingsPages.personaMemory.loadFailed') }}
			</p>
			<Button variant="outline" size="sm" @click="load">{{
				$t('settingsPages.personaMemory.retry')
			}}</Button>
		</div>
		<div
			v-else-if="entries.length === 0"
			class="rounded-lg border-2 border-dashed border-line-strong p-8 text-center text-sm text-ink-subtle"
			data-testid="memory-empty"
		>
			{{ $t('settingsPages.personaMemory.empty') }}
		</div>
		<ul v-else class="space-y-3">
			<li
				v-for="entry in entries"
				:key="`${entry.persona_uuid}-${entry.id}`"
				class="rounded-lg border border-line bg-surface p-4"
				data-testid="memory-entry"
			>
				<div class="mb-2 flex flex-wrap items-center gap-2 text-xs">
					<span
						class="rounded bg-muted px-1.5 py-0.5 font-semibold text-ink-subtle uppercase"
					>
						{{
							entry.kind === 'journal'
								? $t('settingsPages.personaMemory.journal')
								: topicTitle(entry.topic)
						}}
					</span>
					<span
						:class="[
							'rounded px-1.5 py-0.5 font-semibold',
							entry.author_kind === 'agent'
								? 'bg-violet-100 text-violet-700 dark:bg-violet-900/30 dark:text-violet-300'
								: 'bg-sky-100 text-sky-700 dark:bg-sky-900/30 dark:text-sky-300',
						]"
					>
						{{
							entry.author_kind === 'agent'
								? $t('settingsPages.personaMemory.agent')
								: $t('settingsPages.personaMemory.person')
						}}
					</span>
					<span class="text-ink-subtle">{{
						personaName(entry.persona_uuid)
					}}</span>
				</div>

				<div v-if="editing?.id === entry.id" class="flex flex-col gap-2">
					<select v-model="editing.topic" :class="fieldClass">
						<option v-for="topic in MEMORY_TOPICS" :key="topic" :value="topic">
							{{ topicTitle(topic) }}
						</option>
					</select>
					<Textarea v-model="editing.body" rows="3" />
					<div class="flex gap-2">
						<Button
							size="sm"
							:disabled="busy || !editing.body.trim()"
							@click="saveEdit(entry)"
						>
							{{ $t('settingsPages.personaMemory.save') }}
						</Button>
						<Button variant="outline" size="sm" @click="editing = null">
							{{ $t('settingsPages.personaMemory.cancel') }}
						</Button>
					</div>
				</div>
				<p v-else class="text-sm whitespace-pre-wrap text-ink">
					{{ entry.body }}
				</p>

				<div
					class="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-ink-subtle"
				>
					<router-link
						v-if="entry.source_task_id"
						:to="taskUrl(entry.source_task_id)"
						class="text-brand hover:underline"
					>
						{{
							$t('settingsPages.personaMemory.sourceTask', {
								id: entry.source_task_id,
							})
						}}
					</router-link>
					<span v-if="entry.check_by" class="flex items-center gap-1">
						{{
							$t('settingsPages.personaMemory.checkBy', {
								date: entry.check_by,
							})
						}}
						<span
							v-if="isCheckByStale(entry.check_by)"
							class="rounded bg-amber-100 px-1.5 py-0.5 font-semibold text-amber-800 dark:bg-amber-900/30 dark:text-amber-300"
							data-testid="memory-stale"
						>
							{{ $t('settingsPages.personaMemory.stale') }}
						</span>
					</span>
					<span>{{
						$t('settingsPages.personaMemory.created', {
							date: formatDate(entry.created_at),
						})
					}}</span>
					<span v-if="editing?.id !== entry.id" class="ml-auto flex gap-3">
						<button
							v-if="entry.kind === 'fact'"
							type="button"
							class="hover:text-ink hover:underline"
							@click="startEdit(entry)"
						>
							{{ $t('settingsPages.personaMemory.edit') }}
						</button>
						<button
							type="button"
							class="hover:text-destructive hover:underline"
							@click="forget(entry)"
						>
							{{ $t('settingsPages.personaMemory.forget') }}
						</button>
					</span>
				</div>
			</li>
		</ul>

		<section class="mt-8" data-testid="memory-deleted">
			<button
				type="button"
				class="text-sm font-medium text-ink"
				@click="toggleDeleted"
			>
				{{ $t('settingsPages.personaMemory.deleted') }}
			</button>
			<template v-if="showDeleted">
				<p v-if="deleted.length === 0" class="mt-2 text-sm text-ink-subtle">
					{{ $t('settingsPages.personaMemory.noDeleted') }}
				</p>
				<ul class="space-y-2 mt-2">
					<li
						v-for="entry in deleted"
						:key="`${entry.persona_uuid}-${entry.id}`"
						class="flex items-start justify-between gap-3 rounded-lg border border-line bg-surface-sunken p-3"
					>
						<div class="min-w-0">
							<p class="text-xs text-ink-subtle">
								{{ personaName(entry.persona_uuid) }} ·
								{{ topicTitle(entry.topic) }}
							</p>
							<p class="text-sm whitespace-pre-wrap text-ink-muted">
								{{ entry.body }}
							</p>
						</div>
						<Button
							variant="outline"
							size="sm"
							:disabled="busy"
							@click="restore(entry)"
						>
							{{ $t('settingsPages.personaMemory.restore') }}
						</Button>
					</li>
				</ul>
			</template>
		</section>
	</PageContainer>
</template>

<script lang="ts">
	import {
		addPersonaMemory,
		forgetPersonaMemory,
		getWorkspaceMemoryOverview,
		listDeletedPersonaMemory,
		listPersonaMemory,
		MEMORY_TOPICS,
		memoryErrorMessage,
		restorePersonaMemory,
		updatePersonaMemory,
		type PersonaMemoryEntry,
		type WorkspaceMemoryOverviewRow,
	} from '@/actions/tmgr/personaMemory';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import { Button } from '@/components/ui/button';
	import { Switch } from '@/components/ui/switch';
	import { Textarea } from '@/components/ui/textarea';
	import { toast } from '@/components/ui/toast';
	import { currentLocale, t } from '@/i18n';
	import store from '@/store';
	import { isCheckByStale } from '@/utils/personaMemory';
	import { generateTaskUrl } from '@/utils/url';
	import {
		computed,
		defineComponent,
		onMounted,
		reactive,
		ref,
		watch,
	} from 'vue';
	import { useRoute } from 'vue-router';

	const LIMIT = 50;

	export default defineComponent({
		name: 'WorkspacePersonaMemory',
		components: { PageContainer, PageHeader, Button, Switch, Textarea },
		setup() {
			const route = useRoute();
			const workspace = computed(
				() =>
					store.getters.currentWorkspace as
						{ id: number; code: string } | undefined,
			);
			const workspaceCode = computed(
				() => workspace.value?.code ?? String(route.params.workspace_code),
			);

			const overview = ref<WorkspaceMemoryOverviewRow[]>([]);
			const entries = ref<PersonaMemoryEntry[]>([]);
			const deleted = ref<PersonaMemoryEntry[]>([]);
			const loading = ref(true);
			const failed = ref(false);
			const busy = ref(false);
			const showDeleted = ref(false);
			const adding = ref(false);
			const editing = ref<{ id: number; topic: string; body: string } | null>(
				null,
			);

			const personaFilter = ref(String(route.query.persona ?? ''));
			const topicFilter = ref('');
			const query = ref('');
			const includeJournal = ref(false);
			const draft = reactive({
				persona: '',
				topic: 'notes',
				body: '',
				checkBy: '',
			});

			const fieldClass =
				'rounded-md border border-line bg-surface px-2 py-1.5 text-sm text-ink';

			const personaOptions = computed(() =>
				overview.value.map((row) => row.persona),
			);
			const personaName = (uuid: string) =>
				personaOptions.value.find((p) => p.uuid === uuid)?.name ?? '';
			const topicTitle = (topic: string) =>
				t(`settingsPages.personaMemory.topics.${topic}`);
			const formatDate = (date: string) =>
				new Date(date).toLocaleDateString(currentLocale());
			const taskUrl = (id: number) =>
				generateTaskUrl(id, workspace.value ?? { code: workspaceCode.value });

			const targetPersonas = () =>
				personaFilter.value
					? [personaFilter.value]
					: personaOptions.value.map((p) => p.uuid);

			const load = async () => {
				const ws = workspace.value;
				if (!ws) return;
				loading.value = true;
				failed.value = false;
				try {
					overview.value = await getWorkspaceMemoryOverview(ws.id);
					const lists = await Promise.all(
						targetPersonas().map((uuid) =>
							listPersonaMemory(uuid, ws.id, {
								topic: topicFilter.value,
								query: query.value.trim(),
								journal: includeJournal.value,
								limit: LIMIT,
							}),
						),
					);
					entries.value = lists
						.flat()
						.sort(
							(a, b) => Date.parse(b.created_at) - Date.parse(a.created_at),
						);
					if (showDeleted.value) await loadDeleted();
				} catch {
					failed.value = true;
				} finally {
					loading.value = false;
				}
			};

			const loadDeleted = async () => {
				const ws = workspace.value;
				if (!ws) return;
				const lists = await Promise.all(
					targetPersonas().map((uuid) =>
						listDeletedPersonaMemory(uuid, ws.id, LIMIT).catch(() => []),
					),
				);
				deleted.value = lists
					.flat()
					.sort(
						(a, b) =>
							Date.parse(b.deleted_at ?? b.created_at) -
							Date.parse(a.deleted_at ?? a.created_at),
					);
			};

			const toggleDeleted = async () => {
				showDeleted.value = !showDeleted.value;
				if (showDeleted.value) await loadDeleted();
			};

			const fail = (error: unknown) =>
				toast({ title: memoryErrorMessage(error), variant: 'destructive' });

			const run = async (action: () => Promise<unknown>) => {
				busy.value = true;
				try {
					await action();
					await load();
					return true;
				} catch (error) {
					fail(error);
					return false;
				} finally {
					busy.value = false;
				}
			};

			const startAdd = () => {
				draft.persona =
					personaFilter.value || personaOptions.value[0]?.uuid || '';
				draft.topic = 'notes';
				draft.body = '';
				draft.checkBy = '';
				adding.value = true;
			};

			const submitAdd = async () => {
				const ws = workspace.value;
				if (!ws) return;
				const ok = await run(() =>
					addPersonaMemory(draft.persona, ws.id, {
						topic: draft.topic,
						body: draft.body.trim(),
						...(draft.checkBy ? { check_by: draft.checkBy } : {}),
					}),
				);
				if (ok) adding.value = false;
			};

			const startEdit = (entry: PersonaMemoryEntry) => {
				editing.value = { id: entry.id, topic: entry.topic, body: entry.body };
			};

			const saveEdit = async (entry: PersonaMemoryEntry) => {
				const ws = workspace.value;
				if (!ws || !editing.value) return;
				const { topic, body } = editing.value;
				const ok = await run(() =>
					updatePersonaMemory(entry.persona_uuid, ws.id, entry.id, {
						topic,
						body: body.trim(),
					}),
				);
				if (ok) editing.value = null;
			};

			const forget = async (entry: PersonaMemoryEntry) => {
				const ws = workspace.value;
				if (!ws) return;
				await run(() =>
					forgetPersonaMemory(entry.persona_uuid, ws.id, entry.id),
				);
			};

			const restore = async (entry: PersonaMemoryEntry) => {
				const ws = workspace.value;
				if (!ws) return;
				await run(() =>
					restorePersonaMemory(entry.persona_uuid, ws.id, entry.id),
				);
			};

			const setJournal = (value: boolean) => {
				includeJournal.value = value;
			};

			let timer: ReturnType<typeof setTimeout> | null = null;
			watch([personaFilter, topicFilter, includeJournal], load);
			watch(query, () => {
				if (timer) clearTimeout(timer);
				timer = setTimeout(load, 300);
			});
			watch(() => workspace.value?.id, load);

			onMounted(load);

			return {
				MEMORY_TOPICS,
				workspaceCode,
				entries,
				deleted,
				loading,
				failed,
				busy,
				showDeleted,
				adding,
				editing,
				personaFilter,
				topicFilter,
				query,
				includeJournal,
				draft,
				fieldClass,
				personaOptions,
				personaName,
				topicTitle,
				formatDate,
				taskUrl,
				isCheckByStale,
				load,
				toggleDeleted,
				startAdd,
				submitAdd,
				startEdit,
				saveEdit,
				forget,
				restore,
				setJournal,
			};
		},
	});
</script>
