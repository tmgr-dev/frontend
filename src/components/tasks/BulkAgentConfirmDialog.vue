<template>
	<Dialog :open="true" @update:open="(value) => !value && $emit('close')">
		<DialogContent
			class="max-h-[85vh] overflow-y-auto"
			data-testid="bulk-confirm-dialog"
		>
			<DialogHeader>
				<DialogTitle>Confirm for agent runs</DialogTitle>
				<DialogDescription>
					This is exactly what agents will read from each of these
					{{ taskIds.length }} tasks. Confirm only if you wrote or checked all
					of it.
				</DialogDescription>
			</DialogHeader>
			<p v-if="loading" class="text-sm text-ink-subtle">Loading tasks...</p>
			<p
				v-if="anyInvisible"
				class="rounded border border-amber-500 bg-amber-50 p-2 text-xs text-amber-900 dark:bg-amber-950 dark:text-amber-200"
				data-testid="bulk-confirm-warning"
			>
				Some text contains invisible or direction-changing characters, shown
				highlighted as code points.
			</p>
			<div
				v-if="personaGroups.length || contextGroups.length"
				class="flex flex-col gap-2"
				data-testid="bulk-owner-texts"
			>
				<AgentOwnerTexts
					v-for="group in personaGroups"
					:key="group.key"
					:persona="group.persona"
					:persona-note="`${group.count} ${
						group.count === 1 ? 'task' : 'tasks'
					}`"
				/>
				<AgentOwnerTexts
					v-for="group in contextGroups"
					:key="group.key"
					:context="group.context"
					:rules-note="`${group.count} ${group.count === 1 ? 'task' : 'tasks'}`"
				/>
			</div>
			<div class="flex max-h-[50vh] flex-col gap-3 overflow-y-auto text-sm">
				<div
					v-for="row in rows"
					:key="row.id"
					class="rounded border border-line p-2"
					:data-testid="`bulk-row-${row.id}`"
				>
					<div
						class="text-2xs font-bold uppercase text-ink-subtle"
						:data-testid="`bulk-category-${row.id}`"
					>
						Category: {{ row.category }}
					</div>
					<div
						v-if="row.repositories"
						class="text-2xs font-bold uppercase text-ink-subtle"
					>
						Repositories
					</div>
					<ul
						v-if="row.repositories"
						class="mb-1 font-mono text-xs"
						:data-testid="`bulk-repositories-${row.id}`"
					>
						<li v-for="name in row.repositories" :key="name">{{ name }}</li>
					</ul>
					<pre
						class="whitespace-pre-wrap break-words font-mono text-xs font-bold"
					><template v-for="(piece, i) in row.title" :key="i"><mark v-if="piece.flagged" class="bg-amber-300 text-black">{{ piece.text }}</mark><template v-else>{{ piece.text }}</template></template></pre>
					<pre
						class="mt-1 whitespace-pre-wrap break-words font-mono text-xs"
					><template v-if="!row.description.length">(empty)</template><template v-for="(piece, i) in row.description" :key="i"><mark v-if="piece.flagged" class="bg-amber-300 text-black">{{ piece.text }}</mark><template v-else>{{ piece.text }}</template></template></pre>
					<p
						v-if="results[row.id]"
						class="mt-1 text-xs"
						:class="
							results[row.id].ok
								? 'text-green-700 dark:text-green-400'
								: 'text-red-600 dark:text-red-400'
						"
						:data-testid="`bulk-result-${row.id}`"
					>
						{{
							results[row.id].ok ? 'Confirmed' : describe(results[row.id].code)
						}}
					</p>
				</div>
			</div>
			<p v-if="error" class="text-sm text-red-600 dark:text-red-400">
				{{ error }}
			</p>
			<DialogFooter>
				<Button variant="outline" @click="$emit('close')">{{
					done ? 'Close' : 'Cancel'
				}}</Button>
				<Button
					v-if="!done"
					:disabled="loading || busy || !rows.length"
					data-testid="bulk-confirm-submit"
					@click="submit"
				>
					Confirm {{ rows.length }}
				</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import {
		agentRunErrorCode,
		getTaskApproval,
		type ApprovalContext,
		type ApprovalPersona,
	} from '@/actions/tmgr/agentRuns';
	import { getTask, type Task } from '@/actions/tmgr/tasks';
	import AgentOwnerTexts from '@/components/tasks/AgentOwnerTexts.vue';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { signTaskApprovalsBulk } from '@/utils/agentApprovalClient';
	import type { ShownCoverage } from '@/utils/agentApprovalCoverage';
	import {
		categoryLabel,
		hasInvisible,
		splitInvisible,
	} from '@/utils/agentApprovalPolicy';
	import { computed, defineComponent, onMounted, ref } from 'vue';

	const CODES: Record<string, string> = {
		hash_mismatch: 'Changed since it was shown',
		setting_off: 'Agent runs are off for you here',
		approval_revoked: 'Needs a new confirmation',
		category_mismatch: 'Category changed',
		signature_invalid: 'Signature rejected',
		signed_at_skew: 'Your clock is off',
		key_unknown: 'Signing key not accepted',
		repositories_changed: 'Repositories changed since it was shown',
		invalid_repositories: 'Repositories of this category are not valid',
		persona_changed: 'Persona instructions changed since they were shown',
		context_changed: 'Workspace rules changed since they were shown',
		invalid_persona: 'The persona cannot be signed',
	};

	export default defineComponent({
		name: 'BulkAgentConfirmDialog',
		components: {
			AgentOwnerTexts,
			Button,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
		},
		props: { taskIds: { type: Array as () => number[], required: true } },
		emits: ['close'],
		setup(props) {
			const tasks = ref<Task[]>([]);
			const repositories = ref<Record<number, string[] | null>>({});
			const coverage = ref<Record<number, ShownCoverage>>({});
			const loading = ref(true);
			const busy = ref(false);
			const done = ref(false);
			const error = ref('');
			const results = ref<Record<number, { ok: boolean; code?: string }>>({});

			onMounted(async () => {
				try {
					const loaded: Task[] = [];
					const queue = [...props.taskIds];
					await Promise.all(
						Array.from({ length: 6 }, async () => {
							while (queue.length) {
								const id = queue.shift()!;
								const approval = await getTaskApproval(id);
								repositories.value[id] = approval.repositories ?? null;
								coverage.value[id] = {
									persona: approval.persona ?? null,
									context: approval.context ?? null,
								};
								loaded.push(await getTask(id, false));
							}
						}),
					);
					const order = new Map(props.taskIds.map((id, i) => [id, i]));
					tasks.value = loaded.sort(
						(a, b) => order.get(a.id!)! - order.get(b.id!)!,
					);
				} catch (e) {
					error.value = e instanceof Error ? e.message : String(e);
				} finally {
					loading.value = false;
				}
			});

			const rows = computed(() =>
				tasks.value.map((task) => ({
					id: task.id as number,
					repositories: repositories.value[task.id as number] ?? null,
					category: categoryLabel(task as any),
					title: splitInvisible(task.title ?? ''),
					description: splitInvisible(task.description ?? ''),
				})),
			);
			const personaGroups = computed(() => {
				const groups = new Map<
					string,
					{ key: string; persona: ApprovalPersona; count: number }
				>();
				for (const task of tasks.value) {
					const persona = coverage.value[task.id as number]?.persona;
					if (!persona) continue;
					const key = JSON.stringify([
						persona.id,
						persona.name,
						persona.instructions,
					]);
					const group = groups.get(key);
					if (group) group.count++;
					else groups.set(key, { key, persona, count: 1 });
				}
				return [...groups.values()];
			});
			const contextGroups = computed(() => {
				const groups = new Map<
					string,
					{ key: string; context: ApprovalContext; count: number }
				>();
				for (const task of tasks.value) {
					const context = coverage.value[task.id as number]?.context;
					if (!context) continue;
					const key = JSON.stringify([
						task.workspace_id,
						context.pages.map((p) => [p.id, p.title, p.body]),
					]);
					const group = groups.get(key);
					if (group) group.count++;
					else groups.set(key, { key, context, count: 1 });
				}
				return [...groups.values()];
			});
			const anyInvisible = computed(
				() =>
					tasks.value.some(
						(t) =>
							hasInvisible(t.title ?? '') || hasInvisible(t.description ?? ''),
					) ||
					personaGroups.value.some(
						(g) =>
							hasInvisible(g.persona.name) ||
							hasInvisible(g.persona.instructions),
					) ||
					contextGroups.value.some((g) =>
						g.context.pages.some(
							(p) => hasInvisible(p.title) || hasInvisible(p.body),
						),
					),
			);

			const submit = async () => {
				busy.value = true;
				error.value = '';
				try {
					const out = await signTaskApprovalsBulk(
						tasks.value.map((task) => ({
							id: task.id!,
							title: task.title,
							description: task.description,
							workspace_id: task.workspace_id,
							project_category_id: task.project_category_id,
							category: task.category as any,
						})),
						repositories.value,
						coverage.value,
					);
					results.value = Object.fromEntries(out.map((r) => [r.task_id, r]));
					done.value = true;
				} catch (e) {
					const code = agentRunErrorCode(e);
					error.value =
						(code && CODES[code]) ||
						(e instanceof Error ? e.message : String(e));
				} finally {
					busy.value = false;
				}
			};

			const describe = (code?: string) =>
				(code && CODES[code]) || code || 'Failed';

			return {
				loading,
				busy,
				done,
				error,
				rows,
				results,
				anyInvisible,
				personaGroups,
				contextGroups,
				submit,
				describe,
			};
		},
	});
</script>
