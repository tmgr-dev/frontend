<template>
	<section
		class="flex flex-col gap-3 rounded-md border border-border p-4"
		data-testid="local-skills-library"
	>
		<header class="flex flex-col gap-1">
			<h4 class="text-sm font-semibold">
				Skills on this device (local workspaces only)
			</h4>
			<p class="text-xs text-muted-foreground">
				This library is stored on this machine and used by local personas
				through the local MCP. It is not synced to your account.
			</p>
		</header>

		<div class="flex flex-wrap gap-2">
			<Button size="sm" :disabled="!ready" @click="startCreate">
				New skill
			</Button>
			<Button
				size="sm"
				variant="outline"
				:disabled="!ready"
				@click="showImport = true"
			>
				Import from local agents
			</Button>
			<Button
				size="sm"
				variant="outline"
				:disabled="!ready || checking || !hasLinks"
				@click="checkUpdates"
			>
				{{ checking ? 'Checking…' : 'Check for updates' }}
			</Button>
			<Button
				size="sm"
				variant="outline"
				:disabled="!ready || !skills.length"
				@click="doExport"
			>
				Export
			</Button>
			<Button size="sm" variant="outline" :disabled="!ready" @click="doImport">
				Import
			</Button>
		</div>

		<p v-if="skippedNote" class="text-xs text-amber-800 dark:text-amber-300">
			{{ skippedNote }}
		</p>
		<p v-if="loadError" class="text-sm text-red-600 dark:text-red-400">
			{{ loadError }}
		</p>

		<form
			v-if="editing"
			class="flex flex-col gap-2 rounded border border-border p-3"
			@submit.prevent="save"
		>
			<textarea
				v-model="draft"
				rows="12"
				spellcheck="false"
				class="rounded border border-border bg-background px-2 py-1 font-mono text-xs"
				data-testid="local-skill-markdown"
			/>
			<ul
				v-if="feedback.length"
				class="list-disc pl-4 text-xs text-amber-800 dark:text-amber-300"
			>
				<li v-for="line in feedback" :key="line">{{ line }}</li>
			</ul>
			<p v-if="saveError" class="text-sm text-red-600 dark:text-red-400">
				{{ saveError }}
			</p>
			<div class="flex gap-2">
				<Button
					type="submit"
					size="sm"
					:disabled="saving || feedback.length > 0"
				>
					{{ saving ? 'Saving…' : 'Save skill' }}
				</Button>
				<Button type="button" size="sm" variant="outline" @click="cancelEdit">
					Cancel
				</Button>
			</div>
		</form>

		<p v-if="ready && !skills.length" class="text-sm text-muted-foreground">
			No skills yet.
		</p>
		<div
			v-for="skill in skills"
			:key="skill.slug"
			class="flex items-start justify-between gap-3 rounded border border-border p-3"
			:data-testid="`local-skill-${skill.slug}`"
		>
			<div class="min-w-0">
				<p class="flex flex-wrap items-center gap-2 text-sm font-medium">
					{{ skill.title }}
					<span
						v-if="statuses[skill.slug] === 'update-available'"
						class="rounded bg-blue-100 px-1.5 py-0.5 text-2xs text-blue-900 dark:bg-blue-900/30 dark:text-blue-200"
					>
						Update available
					</span>
					<span
						v-else-if="statuses[skill.slug] === 'edited-and-changed'"
						class="rounded bg-amber-100 px-1.5 py-0.5 text-2xs text-amber-900 dark:bg-amber-900/30 dark:text-amber-200"
					>
						Edited here and changed at source
					</span>
					<span
						v-else-if="statuses[skill.slug] === 'not-scanned'"
						class="rounded bg-muted px-1.5 py-0.5 text-2xs text-muted-foreground"
					>
						Source not scanned
					</span>
				</p>
				<p class="text-xs text-muted-foreground">
					{{ skill.slug }} · v{{ skill.version }}
				</p>
				<p class="text-xs text-muted-foreground">
					Attached to: {{ attachedNames(skill.slug) }}
				</p>
			</div>
			<div class="flex shrink-0 gap-2">
				<Button size="sm" variant="outline" @click="startEdit(skill)">
					Edit
				</Button>
				<Button size="sm" variant="outline" @click="remove(skill)">
					Delete
				</Button>
			</div>
		</div>

		<SkillImportDialog
			v-if="showImport && target"
			v-model:open="showImport"
			:target="target"
			@imported="reload"
		/>
	</section>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import { toast } from '@/components/ui/toast';
	import { LOCAL_OWNER_ID } from '@/local/personaGate';
	import { listLocalPersonas } from '@/local/personas';
	import { activeLocalWorkspace, localContext } from '@/local/runtime';
	import {
		deleteSkill,
		importSkills,
		putSkill,
		SkillLibraryError,
		type LibraryFile,
		type LibrarySkill,
	} from '@/local/skillLibrary';
	import { localSkillTarget } from '@/local/skillLibraryTarget';
	import {
		linkedUpdateStatuses,
		type SkillUpdateStatus,
	} from '@/local/skillUpdateStatus';
	import { skillLibraryRevision } from '@/local/skillLibraryRevision';
	import {
		exportSkillLibrary,
		pickSkillLibraryImport,
		readSkillLibraryReport,
		updateSkillLibrary,
	} from '@/local/tauriSkillLibrary';
	import SkillImportDialog from '@/pages/Settings/SkillImportDialog.vue';
	import {
		parseSkillFrontMatter,
		SKILL_TEMPLATE,
		validateSkillMarkdown,
	} from '@/utils/personas';
	import type { ScanResult } from '@/utils/skillImport';
	import { computed, defineComponent, onMounted, ref, watch } from 'vue';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'LocalSkillsLibrary',
		components: { Button, SkillImportDialog },
		setup() {
			const store = useStore();
			const userId = () =>
				store.getters?.isLocalOnly
					? LOCAL_OWNER_ID
					: Number(store.state.user?.id) || 0;
			const ready = computed(() => userId() !== 0);
			const target = computed(() =>
				ready.value ? localSkillTarget(userId()) : null,
			);

			const library = ref<LibraryFile | null>(null);
			const loadError = ref<string | null>(null);
			const skippedNote = ref<string | null>(null);
			const personaNames = ref<Record<string, string>>({});
			const statuses = ref<Record<string, SkillUpdateStatus>>({});
			const checking = ref(false);
			const showImport = ref(false);

			const editing = ref(false);
			const editingSlug = ref<string | null>(null);
			const draft = ref('');
			const saving = ref(false);
			const saveError = ref<string | null>(null);

			const skills = computed(() => library.value?.skills ?? []);
			const hasLinks = computed(
				() => Object.keys(library.value?.links ?? {}).length > 0,
			);

			const feedback = computed(() => {
				const parsed = parseSkillFrontMatter(draft.value);
				const errors = validateSkillMarkdown(
					draft.value,
					editingSlug.value ?? parsed?.slug ?? '',
				);
				return errors
					? Object.entries(errors).map(
							([field, messages]) => `${field} ${messages.join(', ')}`,
						)
					: [];
			});

			const reload = async () => {
				if (!ready.value) return;
				try {
					const report = await readSkillLibraryReport(userId());
					library.value = report.library;
					skippedNote.value = report.skipped.length
						? `Skipped ${report.skipped.length} unreadable skill(s): ${report.skipped
								.map((s) => s.slug || 'entry')
								.join(', ')}`
						: null;
					statuses.value = {};
					loadError.value = null;
				} catch (error) {
					loadError.value =
						error instanceof Error ? error.message : String(error);
				}
			};

			const loadPersonaNames = async () => {
				const workspace = activeLocalWorkspace();
				if (!workspace) return;
				try {
					const ctx = await localContext(workspace, {
						id: userId(),
						name: store.state.user?.name ?? '',
						email: '',
					});
					personaNames.value = Object.fromEntries(
						(await listLocalPersonas(ctx)).map((p) => [p.uuid, p.name]),
					);
				} catch {
					personaNames.value = {};
				}
			};

			onMounted(() => {
				reload();
				loadPersonaNames();
			});
			watch(userId, () => {
				reload();
				loadPersonaNames();
			});
			watch(skillLibraryRevision, reload);

			const attachedNames = (slug: string) => {
				const names = (library.value?.attachments ?? [])
					.filter((a) => a.slug === slug)
					.map(
						(a) => personaNames.value[a.persona_uuid] ?? 'another workspace',
					);
				return names.length ? names.join(', ') : 'no persona';
			};

			const startCreate = () => {
				editingSlug.value = null;
				draft.value = SKILL_TEMPLATE;
				saveError.value = null;
				editing.value = true;
			};
			const startEdit = (skill: LibrarySkill) => {
				editingSlug.value = skill.slug;
				draft.value = skill.markdown;
				saveError.value = null;
				editing.value = true;
			};
			const cancelEdit = () => {
				editing.value = false;
			};

			const save = async () => {
				const slug =
					editingSlug.value ?? parseSkillFrontMatter(draft.value)?.slug;
				if (!slug) return;
				saving.value = true;
				saveError.value = null;
				try {
					await updateSkillLibrary(userId(), (lib) => {
						if (!editingSlug.value && lib.skills.some((s) => s.slug === slug)) {
							throw new SkillLibraryError(`skill ${slug} already exists`);
						}
						return {
							library: putSkill(
								lib,
								slug,
								draft.value,
								new Date().toISOString(),
							).library,
							result: null,
						};
					});
					editing.value = false;
					await reload();
				} catch (error) {
					saveError.value =
						error instanceof Error ? error.message : String(error);
				} finally {
					saving.value = false;
				}
			};

			const remove = async (skill: LibrarySkill) => {
				const attached = (library.value?.attachments ?? []).filter(
					(a) => a.slug === skill.slug,
				).length;
				const note = attached
					? ` It will be detached from ${attached} persona(s).`
					: '';
				if (!window.confirm(`Delete skill "${skill.title}"?${note}`)) return;
				try {
					await updateSkillLibrary(userId(), (lib) => ({
						library: deleteSkill(lib, skill.slug),
						result: null,
					}));
					await reload();
				} catch (error) {
					toast({
						title: 'Could not delete the skill',
						description: error instanceof Error ? error.message : String(error),
						variant: 'destructive',
					});
				}
			};

			const doExport = async () => {
				try {
					const saved = await exportSkillLibrary(userId());
					toast({ title: saved ? 'Skills exported' : 'Export cancelled' });
				} catch (error) {
					toast({
						title: 'Could not export the skills',
						description: error instanceof Error ? error.message : String(error),
						variant: 'destructive',
					});
				}
			};

			const doImport = async () => {
				try {
					const incoming = await pickSkillLibraryImport();
					if (!incoming) {
						toast({ title: 'Import cancelled' });
						return;
					}
					const outcome = await updateSkillLibrary(userId(), (lib) => {
						const result = importSkills(
							lib,
							incoming,
							new Date().toISOString(),
						);
						return { library: result.library, result };
					});
					const reasons = outcome.skipped
						.map((s) => `${s.slug || 'entry'}: ${s.reason}`)
						.join('; ');
					toast({
						title: `Imported ${outcome.imported.length}, skipped ${outcome.skipped.length}`,
						description: reasons || undefined,
						variant: outcome.skipped.length ? 'destructive' : undefined,
					});
					await reload();
				} catch (error) {
					toast({
						title: 'Could not import the skills',
						description: error instanceof Error ? error.message : String(error),
						variant: 'destructive',
					});
				}
			};

			const checkUpdates = async () => {
				if (!library.value) return;
				checking.value = true;
				try {
					const { invoke } = await import('@tauri-apps/api/core');
					const scan = await invoke<ScanResult>('skills_scan', {
						folderIds: [],
					});
					statuses.value = linkedUpdateStatuses(library.value, scan);
					if (!Object.keys(statuses.value).length) {
						toast({ title: 'All linked skills are up to date' });
					}
				} catch {
					toast({
						title: 'Could not scan this computer',
						variant: 'destructive',
					});
				} finally {
					checking.value = false;
				}
			};

			return {
				ready,
				target,
				loadError,
				skippedNote,
				skills,
				hasLinks,
				statuses,
				checking,
				showImport,
				editing,
				draft,
				feedback,
				saving,
				saveError,
				attachedNames,
				startCreate,
				startEdit,
				cancelEdit,
				save,
				remove,
				doExport,
				doImport,
				checkUpdates,
				reload,
			};
		},
	});
</script>
