<template>
	<div class="flex flex-col gap-4" data-testid="persona-memory">
		<section class="flex flex-col gap-2">
			<h5 class="text-sm font-medium">
				{{ $t('settingsPages.personaMemory.aboutMe') }}
			</h5>
			<Textarea
				v-model="selfBody"
				rows="4"
				:disabled="selfLoading || selfSaving"
				data-testid="persona-memory-self"
			/>
			<div class="flex flex-wrap items-center justify-between gap-2">
				<p
					:class="[
						'text-xs',
						tooLong ? 'text-destructive' : 'text-muted-foreground',
					]"
					data-testid="persona-memory-bytes"
				>
					{{
						$t('settingsPages.personaMemory.aboutMeBytes', {
							used: bytes,
							max: SELF_MEMORY_MAX_BYTES,
						})
					}}
				</p>
				<Button
					size="sm"
					:disabled="selfLoading || selfSaving || tooLong || !dirty"
					@click="saveSelf"
				>
					{{ $t('settingsPages.personaMemory.save') }}
				</Button>
			</div>
			<p class="text-xs text-muted-foreground">
				{{ $t('settingsPages.personaMemory.aboutMeHint') }}
			</p>
		</section>

		<section class="flex flex-col gap-2 border-t border-border pt-3">
			<h5 class="text-sm font-medium">
				{{ $t('settingsPages.personaMemory.byWorkspace') }}
			</h5>
			<p v-if="loading" class="text-xs text-muted-foreground">
				{{ $t('settingsPages.personaMemory.loading') }}
			</p>
			<p v-else-if="rows.length === 0" class="text-xs text-muted-foreground">
				{{ $t('settingsPages.personaMemory.noWorkspaces') }}
			</p>
			<div
				v-for="row in rows"
				:key="row.workspace.id"
				class="flex flex-wrap items-center justify-between gap-2 rounded border border-border p-2"
				data-testid="persona-memory-workspace"
			>
				<div class="min-w-0">
					<p class="truncate text-sm font-medium">{{ row.workspace.name }}</p>
					<p class="text-xs text-muted-foreground">
						{{ topicSummary(row.topics) }}
					</p>
				</div>
				<router-link
					:to="{
						path: `/${row.workspace.code}/persona-memory`,
						query: { persona: persona.id },
					}"
					class="text-xs text-primary hover:underline"
				>
					{{ $t('settingsPages.personaMemory.openMemory') }}
				</router-link>
			</div>
		</section>

		<section class="flex flex-col gap-2 border-t border-border pt-3">
			<h5 class="text-sm font-medium">
				{{ $t('settingsPages.personaMemory.recentWork') }}
			</h5>
			<p v-if="recent.length === 0" class="text-xs text-muted-foreground">
				{{ $t('settingsPages.personaMemory.noRecentWork') }}
			</p>
			<div
				v-for="entry in recent"
				:key="`${entry.workspaceId}-${entry.id}`"
				class="rounded border border-border p-2"
				data-testid="persona-memory-recent"
			>
				<p class="text-sm whitespace-pre-wrap">{{ entry.body }}</p>
				<p class="mt-1 text-xs text-muted-foreground">
					{{ entry.workspaceName }} · {{ formatDate(entry.created_at) }}
				</p>
			</div>
		</section>
	</div>
</template>

<script lang="ts">
	import {
		getSelfMemory,
		getWorkspaceMemoryOverview,
		memoryErrorMessage,
		putSelfMemory,
		type PersonaMemoryEntry,
	} from '@/actions/tmgr/personaMemory';
	import { listWorkspacePersonas, type Persona } from '@/actions/tmgr/personas';
	import { Button } from '@/components/ui/button';
	import { Textarea } from '@/components/ui/textarea';
	import { toast } from '@/components/ui/toast';
	import { currentLocale, t } from '@/i18n';
	import store from '@/store';
	import {
		SELF_MEMORY_MAX_BYTES,
		selfMemoryTooLong,
	} from '@/utils/personaMemory';
	import { byteLength } from '@/utils/personas';
	import { computed, defineComponent, onMounted, ref, type PropType } from 'vue';

	interface WorkspaceRef {
		id: number;
		code: string;
		name: string;
		is_local?: boolean;
	}

	interface MemoryRow {
		workspace: WorkspaceRef;
		topics: Record<string, number>;
	}

	type RecentEntry = PersonaMemoryEntry & {
		workspaceId: number;
		workspaceName: string;
	};

	const RECENT_LIMIT = 5;

	export default defineComponent({
		name: 'PersonaMemory',
		components: { Button, Textarea },
		props: {
			persona: { type: Object as PropType<Persona>, required: true },
		},
		setup(props) {
			const selfBody = ref('');
			const savedBody = ref('');
			const selfLoading = ref(true);
			const selfSaving = ref(false);
			const loading = ref(true);
			const rows = ref<MemoryRow[]>([]);
			const recent = ref<RecentEntry[]>([]);

			const bytes = computed(() => byteLength(selfBody.value));
			const tooLong = computed(() => selfMemoryTooLong(selfBody.value));
			const dirty = computed(() => selfBody.value !== savedBody.value);

			const loadSelf = async () => {
				try {
					savedBody.value = await getSelfMemory(props.persona.id);
					selfBody.value = savedBody.value;
				} catch (error) {
					toast({
						title: memoryErrorMessage(error),
						variant: 'destructive',
					});
				} finally {
					selfLoading.value = false;
				}
			};

			const saveSelf = async () => {
				selfSaving.value = true;
				try {
					savedBody.value = await putSelfMemory(
						props.persona.id,
						selfBody.value,
					);
					toast({ title: t('settingsPages.personaMemory.saved') });
				} catch (error) {
					toast({
						title: memoryErrorMessage(error),
						variant: 'destructive',
					});
				} finally {
					selfSaving.value = false;
				}
			};

			const loadWorkspaces = async () => {
				const workspaces = (
					(store.state.workspaces as WorkspaceRef[]) || []
				).filter(
					(w) =>
						!w.is_local &&
						(!props.persona.workspace_id || w.id === props.persona.workspace_id),
				);
				const collected: MemoryRow[] = [];
				const journal: RecentEntry[] = [];
				await Promise.all(
					workspaces.map(async (workspace) => {
						try {
							const grants = await listWorkspacePersonas(workspace.id);
							const grant = grants.find((g) => g.persona.id === props.persona.id);
							if (!grant || grant.blocked) return;
							const overview = await getWorkspaceMemoryOverview(workspace.id);
							const mine = overview.find(
								(o) => o.persona.uuid === props.persona.id,
							);
							collected.push({ workspace, topics: mine?.topics ?? {} });
							for (const entry of mine?.recent ?? []) {
								if (entry.kind !== 'journal') continue;
								journal.push({
									...entry,
									workspaceId: workspace.id,
									workspaceName: workspace.name,
								});
							}
						} catch {
							return;
						}
					}),
				);
				rows.value = collected.sort((a, b) => a.workspace.id - b.workspace.id);
				recent.value = journal
					.sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at))
					.slice(0, RECENT_LIMIT);
				loading.value = false;
			};

			const topicSummary = (topics: Record<string, number>) => {
				const parts = Object.entries(topics)
					.filter(([, count]) => count > 0)
					.map(
						([topic, count]) =>
							`${t(`settingsPages.personaMemory.topics.${topic}`)} ${count}`,
					);
				return parts.length
					? parts.join(' · ')
					: t('settingsPages.personaMemory.noTopics');
			};

			const formatDate = (date: string) =>
				new Date(date).toLocaleDateString(currentLocale());

			onMounted(() => {
				loadSelf();
				loadWorkspaces();
			});

			return {
				selfBody,
				selfLoading,
				selfSaving,
				loading,
				rows,
				recent,
				bytes,
				tooLong,
				dirty,
				SELF_MEMORY_MAX_BYTES,
				saveSelf,
				topicSummary,
				formatDate,
			};
		},
	});
</script>
