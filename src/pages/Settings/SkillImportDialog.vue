<template>
	<Dialog :open="open" @update:open="onOpenChange">
		<DialogContent class="sm:max-w-[920px]" data-testid="skill-import">
			<DialogHeader>
				<DialogTitle>Import skills from local agents</DialogTitle>
			</DialogHeader>
			<p class="text-xs text-muted-foreground">
				Read-only scan of skill folders on this computer. Nothing is executed;
				only the skills you tick are sent to your skills library.
			</p>

			<p v-if="scanning" class="text-sm text-muted-foreground">Scanning…</p>
			<p v-else-if="scanFailed" class="text-sm text-destructive">
				Could not scan this computer.
			</p>
			<div v-else class="grid min-h-[320px] grid-cols-2 gap-3">
				<div class="flex max-h-[420px] flex-col gap-3 overflow-y-auto pr-1">
					<p
						v-if="rows.length === 0"
						class="text-sm text-muted-foreground"
						data-testid="skill-import-empty"
					>
						No skills found. Use “Add folder…” to scan a project.
					</p>
					<div v-for="group in groups" :key="group.id" class="flex flex-col">
						<label class="flex items-center gap-2 text-sm font-semibold">
							<input
								type="checkbox"
								class="accent-primary"
								:checked="groupState(group.rows) === 'all'"
								:indeterminate.prop="groupState(group.rows) === 'some'"
								:disabled="busy || !hasSelectable(group.rows)"
								@change="
									(e) =>
										setGroup(group.rows, (e.target as HTMLInputElement).checked)
								"
							/>
							{{ group.label }}
						</label>
						<div
							v-for="row in group.rows"
							:key="row.skill.key"
							:class="[
								'ml-5 flex cursor-pointer items-start gap-2 rounded px-1 py-1 text-sm',
								selectedKey === row.skill.key
									? 'bg-muted'
									: 'hover:bg-muted/50',
							]"
							:data-testid="`skill-import-row-${row.skill.key}`"
							@click="selectedKey = row.skill.key"
						>
							<input
								type="checkbox"
								class="mt-1 accent-primary"
								:checked="row.ticked"
								:disabled="busy || !selectable(row)"
								@click.stop
								@change="
									(e) => (row.ticked = (e.target as HTMLInputElement).checked)
								"
							/>
							<span class="min-w-0 flex-1">
								<span class="block truncate">{{ row.converted.title }}</span>
								<span
									:class="[
										'w-fit rounded px-1.5 py-0.5 text-2xs',
										statusClass(row.status),
									]"
								>
									{{ statusLabel(row) }}
								</span>
								<span
									v-if="resultFor(row)"
									:class="[
										'block text-2xs',
										resultFor(row)!.ok
											? 'text-green-700 dark:text-green-400'
											: 'text-destructive',
									]"
								>
									{{ resultFor(row)!.ok ? 'Imported' : resultFor(row)!.error }}
								</span>
							</span>
						</div>
					</div>
				</div>

				<div
					class="flex max-h-[420px] flex-col gap-2 overflow-y-auto border-l border-border pl-3"
					data-testid="skill-import-preview"
				>
					<template v-if="selected">
						<p class="text-xs break-all text-muted-foreground">
							{{ selected.skill.path }}
							<template v-if="selected.skill.symlink_target">
								→ {{ selected.skill.symlink_target }}
							</template>
						</p>
						<p
							v-if="selected.converted.blockedReason"
							class="text-xs text-destructive"
						>
							Cannot import: {{ selected.converted.blockedReason }}
						</p>
						<label class="flex flex-col gap-1 text-xs">
							Library slug
							<input
								class="rounded border border-border bg-background px-2 py-1 font-mono text-xs"
								:value="selected.converted.slug"
								:disabled="busy || isTracked(selected)"
								data-testid="skill-import-slug"
								@change="
									(e) =>
										setSlug(selected!, (e.target as HTMLInputElement).value)
								"
							/>
						</label>
						<button
							v-if="selected.status === 'slug-taken'"
							type="button"
							class="w-fit text-xs text-primary underline"
							@click="setSlug(selected!, clashSuggestion(selected!))"
						>
							Use {{ clashSuggestion(selected) }} instead
						</button>
						<ul
							v-if="selected.converted.warnings.length"
							class="list-disc pl-4 text-xs text-amber-800 dark:text-amber-300"
						>
							<li v-for="w in selected.converted.warnings" :key="w">{{ w }}</li>
						</ul>
						<p
							v-if="selected.converted.droppedFields.length"
							class="text-xs text-muted-foreground"
						>
							Not carried over:
							{{ selected.converted.droppedFields.join(', ') }}
						</p>
						<div
							v-if="selected.skill.supporting.length"
							class="text-xs text-muted-foreground"
						>
							Supporting files (not imported):
							{{ selected.skill.supporting.map((f) => f.name).join(', ')
							}}<template v-if="selected.skill.supporting_more">
								and {{ selected.skill.supporting_more }} more</template
							>
						</div>
						<pre
							class="rounded border border-border bg-muted/40 p-2 font-mono text-2xs break-words whitespace-pre-wrap"
							data-testid="skill-import-markdown"
							>{{ selected.converted.markdown }}</pre>
					</template>
					<p v-else class="text-sm text-muted-foreground">
						Select a skill to preview it.
					</p>
				</div>
			</div>

			<DialogFooter class="items-center sm:justify-between">
				<Button
					variant="outline"
					size="sm"
					data-testid="skill-import-add-folder"
					:disabled="busy || scanning"
					@click="addFolder"
				>
					Add folder…
				</Button>
				<div class="flex items-center gap-2">
					<span class="text-xs text-muted-foreground">
						{{ tickedCount }} selected
					</span>
					<Button variant="outline" :disabled="busy" @click="close">
						{{ finished ? 'Close' : 'Cancel' }}
					</Button>
					<Button
						data-testid="skill-import-confirm"
						:disabled="busy || scanning || tickedCount === 0"
						@click="confirm"
					>
						Import selected
					</Button>
				</div>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import {
		getSharedSkill,
		listSharedSkills,
		putSharedSkill,
	} from '@/actions/tmgr/personas';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { toast } from '@/components/ui/toast';
	import store from '@/store';
	import {
		defaultTicked,
		importSummary,
		indexStorageKey,
		loadIndex,
		saveIndex,
		type ImportIndex,
		type LibraryEntryRef,
		type RowStatus,
		type ScanResult,
	} from '@/utils/skillImport';
	import {
		adoptMatches,
		buildRows,
		clashSuggestion,
		evaluateRow,
		runImport,
		type ImportRow,
		type RowResult,
	} from '@/utils/skillImportFlow';
	import { computed, defineComponent, ref, watch } from 'vue';

	const STATUS_LABELS: Record<RowStatus, string> = {
		new: 'New',
		'up-to-date': 'Up to date',
		changed: 'Changed since last import — will update',
		'edited-in-library': 'Edited in library since import — will overwrite',
		'slug-taken': 'Slug taken in library — tick to overwrite',
		'previously-imported-deleted': 'Imported before, deleted from library',
		blocked: 'Cannot import',
	};

	const STATUS_CLASSES: Record<RowStatus, string> = {
		new: 'bg-muted text-muted-foreground',
		'up-to-date': 'bg-muted text-muted-foreground',
		changed: 'bg-blue-100 text-blue-900 dark:bg-blue-900/30 dark:text-blue-200',
		'edited-in-library':
			'bg-amber-100 text-amber-900 dark:bg-amber-900/30 dark:text-amber-200',
		'slug-taken':
			'bg-amber-100 text-amber-900 dark:bg-amber-900/30 dark:text-amber-200',
		'previously-imported-deleted': 'bg-muted text-muted-foreground',
		blocked: 'bg-red-100 text-red-900 dark:bg-red-900/30 dark:text-red-200',
	};

	const invokeCommand = async <T,>(
		command: string,
		args?: Record<string, unknown>,
	): Promise<T> => {
		const { invoke } = await import('@tauri-apps/api/core');
		return invoke<T>(command, args);
	};

	export default defineComponent({
		name: 'SkillImportDialog',
		components: {
			Button,
			Dialog,
			DialogContent,
			DialogFooter,
			DialogHeader,
			DialogTitle,
		},
		props: {
			open: { type: Boolean, required: true },
		},
		emits: ['update:open', 'imported'],
		setup(props, { emit }) {
			const rows = ref<ImportRow[]>([]);
			const library = ref<LibraryEntryRef[]>([]);
			const index = ref<ImportIndex>({});
			const scanning = ref(false);
			const scanFailed = ref(false);
			const busy = ref(false);
			const finished = ref(false);
			const selectedKey = ref<string | null>(null);
			const results = ref<Record<string, RowResult>>({});
			const folderIds = ref<string[]>([]);

			const userId = () => Number(store.state.user?.id) || 0;
			const storageKey = () =>
				indexStorageKey(import.meta.env.VITE_API_BASE_URL ?? '', userId());
			let scanToken = 0;
			const toRefs = (list: { slug: string; version: number }[]) =>
				list.map((s) => ({ slug: s.slug, version: s.version }));

			const scan = async () => {
				const token = ++scanToken;
				scanning.value = true;
				scanFailed.value = false;
				finished.value = false;
				results.value = {};
				if (!userId()) {
					scanFailed.value = true;
					scanning.value = false;
					return;
				}
				try {
					const [result, summaries] = await Promise.all([
						invokeCommand<ScanResult>('skills_scan', {
							folderIds: folderIds.value,
						}),
						listSharedSkills(false),
					]);
					const refs = toRefs(summaries);
					const loaded = loadIndex(localStorage, storageKey());
					const built = await buildRows(
						result,
						refs,
						loaded,
						async (slug) => (await getSharedSkill(slug)).body,
					);
					if (token !== scanToken) return;
					library.value = refs;
					index.value = adoptMatches(built, refs, loaded);
					if (index.value !== loaded) {
						saveIndex(localStorage, storageKey(), index.value);
					}
					rows.value = built;
					selectedKey.value = rows.value[0]?.skill.key ?? null;
				} catch {
					if (token === scanToken) scanFailed.value = true;
				} finally {
					if (token === scanToken) scanning.value = false;
				}
			};

			watch(
				() => props.open,
				(isOpen) => {
					if (isOpen) scan();
				},
				{ immediate: true },
			);

			const groups = computed(() => {
				const map = new Map<
					string,
					{ id: string; label: string; rows: ImportRow[] }
				>();
				for (const row of rows.value) {
					const id = `${row.sourceId}/${row.packageName}`;
					if (!map.has(id)) {
						map.set(id, {
							id,
							label: `${row.sourceLabel} / ${row.packageName}`,
							rows: [],
						});
					}
					map.get(id)!.rows.push(row);
				}
				return [...map.values()];
			});

			const selected = computed(
				() =>
					rows.value.find((row) => row.skill.key === selectedKey.value) ?? null,
			);
			const tickedCount = computed(
				() => rows.value.filter((row) => row.ticked && selectable(row)).length,
			);

			const selectable = (row: ImportRow) =>
				row.status !== 'blocked' && row.status !== 'up-to-date';
			const hasSelectable = (list: ImportRow[]) => list.some(selectable);
			const groupState = (list: ImportRow[]) => {
				const items = list.filter(selectable);
				const on = items.filter((row) => row.ticked).length;
				if (on === 0) return 'none';
				return on === items.length ? 'all' : 'some';
			};
			const setGroup = (list: ImportRow[], on: boolean) => {
				for (const row of list) if (selectable(row)) row.ticked = on;
			};

			const isTracked = (row: ImportRow) => !!index.value[row.skill.key];

			const setSlug = (row: ImportRow, value: string) => {
				row.slugOverride = value.trim();
				row.bodyMatches = undefined;
				const next = evaluateRow(row, library.value, index.value);
				row.converted = next.converted;
				row.status = next.status;
				row.ticked = false;
			};

			const resultFor = (row: ImportRow) => results.value[row.skill.key];

			const addFolder = async () => {
				try {
					const folder = await invokeCommand<{ id: string } | null>(
						'skills_pick_folder',
					);
					if (!folder) return;
					if (!folderIds.value.includes(folder.id)) {
						folderIds.value = [...folderIds.value, folder.id];
					}
					await scan();
				} catch {
					toast({
						title: 'Could not open the folder',
						variant: 'destructive',
					});
				}
			};

			const confirm = async () => {
				if (busy.value) return;
				busy.value = true;
				try {
					const outcome = await runImport(rows.value, {
						listLibrary: async () => toRefs(await listSharedSkills(false)),
						put: (slug, markdown) => putSharedSkill(slug, markdown),
						getBody: async (slug) => (await getSharedSkill(slug)).body,
						index: index.value,
					});
					index.value = outcome.index;
					if (!saveIndex(localStorage, storageKey(), outcome.index)) {
						toast({
							title: 'Could not remember what was imported',
							description:
								'The next import will treat these skills as already in the library.',
							variant: 'destructive',
						});
					}
					const map: Record<string, RowResult> = {};
					for (const r of outcome.results) map[r.skillKey] = r;
					for (const b of outcome.blocked) {
						map[b.skillKey] = {
							skillKey: b.skillKey,
							ok: false,
							error: b.reason,
						};
					}
					results.value = map;
					finished.value = true;
					const failed =
						outcome.results.filter((r) => !r.ok).length +
						outcome.blocked.length;
					toast({
						title: importSummary(outcome.results),
						variant: failed > 0 ? 'destructive' : undefined,
					});
					emit('imported');
					try {
						library.value = toRefs(await listSharedSkills(false));
						for (const row of rows.value) {
							const next = evaluateRow(row, library.value, index.value);
							row.converted = next.converted;
							row.status = next.status;
							row.ticked = !map[row.skill.key]?.ok && defaultTicked(row.status);
						}
					} catch {
						toast({ title: 'Could not refresh the library' });
					}
				} catch {
					toast({
						title: 'Could not import the skills',
						variant: 'destructive',
					});
				} finally {
					busy.value = false;
				}
			};

			const close = () => emit('update:open', false);
			const onOpenChange = (value: boolean) => {
				if (busy.value && !value) return;
				emit('update:open', value);
			};

			return {
				rows,
				groups,
				selected,
				selectedKey,
				scanning,
				scanFailed,
				busy,
				finished,
				tickedCount,
				selectable,
				hasSelectable,
				groupState,
				setGroup,
				isTracked,
				setSlug,
				resultFor,
				addFolder,
				confirm,
				close,
				onOpenChange,
				clashSuggestion,
				statusLabel: (row: ImportRow) => STATUS_LABELS[row.status],
				statusClass: (status: RowStatus) => STATUS_CLASSES[status],
			};
		},
	});
</script>
