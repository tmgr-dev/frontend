<template>
	<div class="flex flex-col gap-3">
		<p v-if="loading" class="text-xs text-muted-foreground">
			{{ $t('settingsPages.personaSkills.loading') }}
		</p>

		<div v-else class="flex flex-col gap-2">
			<p v-if="skills.length === 0" class="text-xs text-muted-foreground">
				{{ $t('settingsPages.personaSkills.empty') }}
			</p>
			<div
				v-for="skill in skills"
				:key="skill.slug"
				class="flex items-center justify-between gap-3 rounded border border-border p-2"
			>
				<div class="min-w-0">
					<p class="truncate text-sm font-medium">{{ skill.title }}</p>
					<p class="truncate text-xs text-muted-foreground">
						{{ skill.slug }} · v{{ skill.version }} · {{ skill.when }}
					</p>
				</div>
				<div class="flex shrink-0 gap-2">
					<Button
						v-if="!isLocalOnly"
						variant="outline"
						size="sm"
						:data-testid="`convert-skill-${skill.slug}`"
						@click="convertSlug = skill.slug"
					>
						{{ $t('settingsPages.personaSkills.convert') }}
					</Button>
					<Button variant="outline" size="sm" @click="startEdit(skill)">
						{{ $t('common.actions.edit') }}
					</Button>
					<Button variant="outline" size="sm" @click="remove(skill)">
						{{ $t('common.actions.delete') }}
					</Button>
				</div>
			</div>
		</div>

		<p class="text-2xs text-muted-foreground">
			{{
				$t('settingsPages.personaSkills.usedCount', {
					count: skills.length,
					limit: SKILL_LIMIT,
				})
			}}
		</p>

		<div class="flex flex-col gap-2 border-t border-border pt-3">
			<h5 class="text-xs font-semibold">
				{{
					editingSlug
						? $t('settingsPages.personaSkills.editing', { slug: editingSlug })
						: $t('settingsPages.personaSkills.newSkill')
				}}
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
				{{
					$t('settingsPages.personaSkills.bytes', {
						used: draftBytes,
						max: SKILL_BODY_MAX_BYTES,
					})
				}}
			</span>
			<ul v-if="draftErrors" class="text-xs text-destructive">
				<li v-for="(messages, field) in draftErrors" :key="field">
					{{ field }}: {{ messages.join(' ') }}
				</li>
			</ul>
			<div class="flex gap-2">
				<Button size="sm" :disabled="saving" @click="submit">
					{{
						editingSlug
							? $t('settingsPages.personaSkills.saveChanges')
							: $t('settingsPages.personaSkills.createSkill')
					}}
				</Button>
				<Button
					v-if="editingSlug"
					type="button"
					variant="outline"
					size="sm"
					@click="cancelEdit"
				>
					{{ $t('common.actions.cancel') }}
				</Button>
			</div>
		</div>

		<PersonaConvertSkillDialog
			v-if="convertSlug"
			:open="true"
			:persona-id="persona.id"
			:persona-name="persona.name"
			:slug="convertSlug"
			@update:open="(value) => !value && (convertSlug = null)"
			@converted="onConverted"
		/>
	</div>
</template>

<script lang="ts">
	import {
		deletePersonaSkill,
		getPersonaSkill,
		listPersonaSkills,
		putPersonaSkill,
		type Persona,
		type PersonaSkillSummary,
	} from '@/actions/tmgr/personas';
	import { Button } from '@/components/ui/button';
	import { Textarea } from '@/components/ui/textarea';
	import { toast } from '@/components/ui/toast';
	import { t } from '@/i18n';
	import store from '@/store';
	import { isDesktopApp } from '@/utils/desktop';
	import {
		byteLength,
		extractFieldErrors,
		SKILL_BODY_MAX_BYTES,
		SKILL_LIMIT,
		SKILL_TEMPLATE,
		validateSkillMarkdown,
	} from '@/utils/personas';
	import {
		computed,
		defineComponent,
		onMounted,
		ref,
		type PropType,
	} from 'vue';
	import PersonaConvertSkillDialog from './PersonaConvertSkillDialog.vue';

	export default defineComponent({
		name: 'PersonaSkills',
		components: { Button, PersonaConvertSkillDialog, Textarea },
		props: {
			persona: { type: Object as PropType<Persona>, required: true },
		},
		emits: ['changed'],
		setup(props, { emit }) {
			const skills = ref<PersonaSkillSummary[]>([]);
			const loading = ref(true);
			const saving = ref(false);
			const editingSlug = ref<string | null>(null);
			const draft = ref(SKILL_TEMPLATE);
			const serverErrors = ref<Record<string, string[]> | null>(null);
			const convertSlug = ref<string | null>(null);
			const isLocalOnly = isDesktopApp() && !!store.getters.isLocalOnly;

			const load = async () => {
				loading.value = true;
				try {
					skills.value = await listPersonaSkills(props.persona.id);
				} catch {
					toast({
						title: t('settingsPages.personaSkills.loadFailed'),
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

			const startEdit = async (skill: PersonaSkillSummary) => {
				try {
					const full = await getPersonaSkill(props.persona.id, skill.slug);
					editingSlug.value = skill.slug;
					draft.value = full.body;
					serverErrors.value = null;
				} catch {
					toast({
						title: t('settingsPages.personaSkills.loadOneFailed'),
						variant: 'destructive',
					});
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
				if (!editingSlug.value && skills.value.length >= SKILL_LIMIT) {
					toast({
						title: t('settingsPages.personaSkills.limitReached', {
							limit: SKILL_LIMIT,
						}),
						variant: 'destructive',
					});
					return;
				}
				saving.value = true;
				serverErrors.value = null;
				try {
					await putPersonaSkill(props.persona.id, targetSlug, draft.value);
					toast({ title: t('settingsPages.personaSkills.saved') });
					cancelEdit();
					await load();
					emit('changed');
				} catch (error) {
					const fieldErrors = extractFieldErrors(error);
					if (fieldErrors) {
						serverErrors.value = fieldErrors.errors;
					} else {
						toast({
							title: t('settingsPages.personaSkills.saveFailed'),
							variant: 'destructive',
						});
					}
				} finally {
					saving.value = false;
				}
			};

			const remove = async (skill: PersonaSkillSummary) => {
				if (
					!window.confirm(
						t('settingsPages.personaSkills.confirmDelete', {
							title: skill.title,
						}),
					)
				)
					return;
				try {
					await deletePersonaSkill(props.persona.id, skill.slug);
					if (editingSlug.value === skill.slug) cancelEdit();
					await load();
					emit('changed');
				} catch {
					toast({
						title: t('settingsPages.personaSkills.deleteFailed'),
						variant: 'destructive',
					});
				}
			};

			const onConverted = async (slug: string) => {
				if (editingSlug.value === slug) cancelEdit();
				convertSlug.value = null;
				await load();
				emit('changed');
			};

			onMounted(load);

			return {
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
				convertSlug,
				isLocalOnly,
				onConverted,
				SKILL_LIMIT,
				SKILL_BODY_MAX_BYTES,
			};
		},
	});
</script>
