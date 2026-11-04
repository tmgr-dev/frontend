<template>
	<div class="flex flex-col gap-3" data-testid="skills-library">
		<div v-if="canImport" class="flex">
			<Button
				variant="outline"
				size="sm"
				data-testid="skill-import-open"
				@click="importOpen = true"
			>
				Import from local agents
			</Button>
		</div>
		<p v-if="loading" class="text-xs text-muted-foreground">Loading library…</p>

		<div v-else class="flex flex-col gap-2">
			<p v-if="skills.length === 0" class="text-xs text-muted-foreground">
				No shared skills yet.
			</p>
			<div
				v-for="skill in skills"
				:key="skill.slug"
				class="flex items-center justify-between gap-3 rounded border border-border p-2"
			>
				<div class="min-w-0">
					<p class="truncate text-sm font-medium">{{ skill.title }}</p>
					<p class="truncate text-xs text-muted-foreground">
						{{ skill.slug }} · v{{ skill.version }} ·
						<template v-if="skill.attached_personas.length === 0">
							Not attached
						</template>
						<template v-else>
							<span
								v-for="(p, i) in skill.attached_personas"
								:key="p.id"
								:title="
									p.overridden
										? 'Overridden by this persona\'s own skill'
										: undefined
								"
							>
								{{ p.name
								}}<template v-if="p.overridden"> (overridden)</template
								><template v-if="i < skill.attached_personas.length - 1"
									>,
								</template>
							</span>
						</template>
					</p>
				</div>
				<div class="flex shrink-0 gap-2">
					<Button variant="outline" size="sm" @click="startEdit(skill)">
						Edit
					</Button>
					<Button variant="outline" size="sm" @click="remove(skill)">
						Delete
					</Button>
				</div>
			</div>
		</div>

		<p class="text-2xs text-muted-foreground">
			{{ skills.length }}/{{ SHARED_SKILL_LIMIT }} used
		</p>

		<div class="flex flex-col gap-2 border-t border-border pt-3">
			<h5 class="text-xs font-semibold">
				{{ editingSlug ? `Edit ${editingSlug}` : 'New shared skill' }}
			</h5>
			<Textarea
				v-model="draft"
				rows="8"
				class="font-mono text-xs"
				placeholder="---&#10;slug: &#10;title: &#10;when: &#10;actions: []&#10;---"
			/>
			<span
				:class="[
					'text-xs',
					draftBytes > SKILL_BODY_MAX_BYTES
						? 'text-destructive'
						: 'text-muted-foreground',
				]"
			>
				{{ draftBytes }} / {{ SKILL_BODY_MAX_BYTES }} bytes
			</span>
			<ul v-if="draftErrors" class="text-xs text-destructive">
				<li v-for="(messages, field) in draftErrors" :key="field">
					{{ field }}: {{ messages.join(' ') }}
				</li>
			</ul>
			<div class="flex gap-2">
				<Button size="sm" :disabled="saving" @click="submit">
					{{ editingSlug ? 'Save changes' : 'Create skill' }}
				</Button>
				<Button
					v-if="editingSlug"
					type="button"
					variant="outline"
					size="sm"
					@click="cancelEdit"
				>
					Cancel
				</Button>
			</div>
		</div>

		<SkillImportDialog
			v-if="canImport"
			v-model:open="importOpen"
			@imported="onImported"
		/>
	</div>
</template>

<script lang="ts">
	import {
		deleteSharedSkill,
		getSharedSkill,
		listSharedSkills,
		putSharedSkill,
		type SharedSkillSummary,
	} from '@/actions/tmgr/personas';
	import { Button } from '@/components/ui/button';
	import { Textarea } from '@/components/ui/textarea';
	import { toast } from '@/components/ui/toast';
	import store from '@/store';
	import { isDesktopApp } from '@/utils/desktop';
	import {
		byteLength,
		detachedSkillsTitle,
		extractFieldErrors,
		SHARED_SKILL_LIMIT,
		SKILL_BODY_MAX_BYTES,
		SKILL_TEMPLATE,
		sharedSkillDeleteConfirm,
		validateSkillMarkdown,
	} from '@/utils/personas';
	import { computed, defineComponent, onMounted, ref, watch } from 'vue';
	import SkillImportDialog from './SkillImportDialog.vue';

	export default defineComponent({
		name: 'SkillsLibrary',
		components: { Button, SkillImportDialog, Textarea },
		props: {
			revision: { type: Number, default: 0 },
		},
		emits: ['changed'],
		setup(props, { emit }) {
			const skills = ref<SharedSkillSummary[]>([]);
			const loading = ref(true);
			const saving = ref(false);
			const editingSlug = ref<string | null>(null);
			const draft = ref(SKILL_TEMPLATE);
			const serverErrors = ref<Record<string, string[]> | null>(null);
			const importOpen = ref(false);
			const canImport = isDesktopApp() && !store.getters.isLocalOnly;

			const load = async () => {
				loading.value = true;
				try {
					skills.value = await listSharedSkills();
				} catch {
					toast({
						title: 'Could not load the skills library',
						variant: 'destructive',
					});
				} finally {
					loading.value = false;
				}
			};

			const draftBytes = computed(() => byteLength(draft.value));
			const draftErrors = computed(
				() =>
					validateSkillMarkdown(draft.value, editingSlug.value ?? '') ??
					serverErrors.value,
			);

			const startEdit = async (skill: SharedSkillSummary) => {
				try {
					const full = await getSharedSkill(skill.slug);
					editingSlug.value = skill.slug;
					draft.value = full.body;
					serverErrors.value = null;
				} catch {
					toast({ title: 'Could not load this skill', variant: 'destructive' });
				}
			};

			const cancelEdit = () => {
				editingSlug.value = null;
				draft.value = SKILL_TEMPLATE;
				serverErrors.value = null;
			};

			const submit = async () => {
				const slugFromDraft =
					/^slug:\s*(.+)$/m
						.exec(draft.value.replace(/\r\n/g, '\n'))?.[1]
						?.trim() ?? '';
				const targetSlug = editingSlug.value ?? slugFromDraft;
				const clientErrors = validateSkillMarkdown(draft.value, targetSlug);
				if (clientErrors) {
					serverErrors.value = clientErrors;
					return;
				}
				if (!editingSlug.value && skills.value.length >= SHARED_SKILL_LIMIT) {
					toast({
						title: `Your library already has ${SHARED_SKILL_LIMIT} skills`,
						variant: 'destructive',
					});
					return;
				}
				saving.value = true;
				serverErrors.value = null;
				try {
					await putSharedSkill(targetSlug, draft.value);
					toast({ title: 'Skill saved' });
					cancelEdit();
					await load();
					emit('changed');
				} catch (error) {
					const fieldErrors = extractFieldErrors(error);
					if (fieldErrors) {
						serverErrors.value = fieldErrors.errors;
						toast({ title: fieldErrors.message, variant: 'destructive' });
					} else {
						toast({ title: 'Could not save skill', variant: 'destructive' });
					}
				} finally {
					saving.value = false;
				}
			};

			const remove = async (skill: SharedSkillSummary) => {
				if (
					!window.confirm(
						sharedSkillDeleteConfirm(skill.title, skill.attached_personas),
					)
				)
					return;
				try {
					const { detached_personas } = await deleteSharedSkill(skill.slug);
					toast({ title: detachedSkillsTitle(detached_personas ?? []) });
					if (editingSlug.value === skill.slug) cancelEdit();
					await load();
					emit('changed');
				} catch {
					toast({ title: 'Could not delete skill', variant: 'destructive' });
				}
			};

			watch(() => props.revision, load);
			onMounted(load);

			const onImported = async () => {
				await load();
				emit('changed');
			};

			return {
				importOpen,
				canImport,
				onImported,
				skills,
				loading,
				saving,
				editingSlug,
				draft,
				draftBytes,
				draftErrors,
				startEdit,
				cancelEdit,
				submit,
				remove,
				SHARED_SKILL_LIMIT,
				SKILL_BODY_MAX_BYTES,
			};
		},
	});
</script>
