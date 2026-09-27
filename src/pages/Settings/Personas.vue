<template>
	<div class="container max-w-4xl py-4">
		<header class="mb-4 flex flex-col gap-1">
			<h3 class="text-lg font-bold">Personas</h3>
			<p class="text-sm text-muted-foreground">
				A persona is an identity your AI agents act under: its own name,
				avatar and system prompt, with its own permissions per workspace.
				{{ activePersonas.length }}/{{ PERSONA_LIMIT }} used.
			</p>
			<label class="mt-1 flex w-fit items-center gap-2 text-sm">
				<Switch :checked="showArchived" @update:checked="toggleShowArchived" />
				Show archived
			</label>
		</header>

		<section class="mb-6 flex flex-col gap-3 rounded-md border border-border p-4">
			<h4 class="text-sm font-semibold">
				{{ editingId ? 'Edit persona' : 'New persona' }}
			</h4>
			<PersonaForm
				:model-value="form"
				:field-errors="formErrors"
				:saving="saving"
				:submit-label="editingId ? 'Save changes' : 'Create persona'"
				@update:model-value="(v) => (form = v)"
				@submit="submitForm"
				@cancel="editingId ? cancelEdit() : null"
				:show-cancel="!!editingId"
			/>
		</section>

		<p v-if="loadError" class="mb-4 text-sm text-red-600 dark:text-red-400">
			{{ loadError }}
		</p>

		<div class="flex flex-col gap-4">
			<article
				v-for="persona in visiblePersonas"
				:key="persona.id"
				class="flex flex-col gap-3 rounded-md border border-border p-4"
			>
				<header class="flex items-start justify-between gap-4">
					<div class="flex items-start gap-3">
						<div class="flex flex-col items-center gap-1">
							<PersonaAvatar
								:uuid="persona.id"
								:name="persona.name"
								:has-avatar="!!persona.avatar_url"
								:size="48"
							/>
							<label
								class="cursor-pointer text-2xs text-primary hover:underline"
							>
								Change
								<input
									type="file"
									accept="image/png,image/jpeg,image/webp,image/gif"
									class="hidden"
									@change="(e) => onAvatarChange(persona, e)"
								/>
							</label>
						</div>
						<div class="flex flex-col gap-1">
							<div class="flex flex-wrap items-center gap-2">
								<h4 class="text-base font-semibold">{{ persona.name }}</h4>
								<span
									v-if="persona.archived_at"
									class="rounded bg-muted px-1.5 py-0.5 text-2xs font-semibold uppercase text-muted-foreground"
								>
									Archived
								</span>
								<span class="text-2xs text-muted-foreground">
									prompt v{{ persona.prompt_version ?? 1 }}
								</span>
							</div>
							<p v-if="persona.description" class="text-sm text-muted-foreground">
								{{ persona.description }}
							</p>
						</div>
					</div>
					<div class="flex gap-2">
						<Button
							v-if="!persona.archived_at"
							variant="outline"
							size="sm"
							@click="startEdit(persona)"
						>
							Edit
						</Button>
						<Button
							v-if="!persona.archived_at"
							variant="outline"
							size="sm"
							@click="archive(persona)"
						>
							Archive
						</Button>
						<Button v-else size="sm" @click="restore(persona)">Restore</Button>
					</div>
				</header>

				<details class="border-t border-border pt-3">
					<summary class="cursor-pointer text-sm font-medium">
						Workspaces
					</summary>
					<PersonaWorkspaceGrants :persona="persona" class="mt-3" />
				</details>

				<details class="border-t border-border pt-3">
					<summary class="cursor-pointer text-sm font-medium">Skills</summary>
					<PersonaSkills :persona="persona" class="mt-3" />
				</details>

				<details class="border-t border-border pt-3">
					<summary class="cursor-pointer text-sm font-medium">
						Agent connections
					</summary>
					<PersonaTokens :persona="persona" class="mt-3" />
				</details>
			</article>

			<p v-if="!loading && visiblePersonas.length === 0" class="text-sm text-muted-foreground">
				No personas yet.
			</p>
		</div>
	</div>
</template>

<script lang="ts">
	import {
		archivePersona as archivePersonaAction,
		createPersona,
		forgetPersonaAvatar,
		listPersonas,
		type Persona,
		type PersonaInput,
		restorePersona as restorePersonaAction,
		updatePersona,
		uploadPersonaAvatar,
	} from '@/actions/tmgr/personas';
	import PersonaAvatar from '@/components/general/PersonaAvatar.vue';
	import { Button } from '@/components/ui/button';
	import { Switch } from '@/components/ui/switch';
	import { toast } from '@/components/ui/toast';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { extractFieldErrors, PERSONA_LIMIT } from '@/utils/personas';
	import { computed, defineComponent, onMounted, ref } from 'vue';
	import PersonaForm, { type PersonaFormModel } from './PersonaForm.vue';
	import PersonaSkills from './PersonaSkills.vue';
	import PersonaTokens from './PersonaTokens.vue';
	import PersonaWorkspaceGrants from './PersonaWorkspaceGrants.vue';

	const emptyForm = (): PersonaFormModel => ({
		name: '',
		description: '',
		system_prompt: '',
	});

	export default defineComponent({
		name: 'PersonasSettings',
		components: {
			Button,
			Switch,
			PersonaAvatar,
			PersonaForm,
			PersonaSkills,
			PersonaTokens,
			PersonaWorkspaceGrants,
		},
		setup() {
			setDocumentTitle('Personas');

			const personas = ref<Persona[]>([]);
			const loading = ref(true);
			const loadError = ref<string | null>(null);
			const showArchived = ref(false);
			const editingId = ref<string | null>(null);
			const form = ref<PersonaFormModel>(emptyForm());
			const formErrors = ref<Record<string, string[]>>({});
			const saving = ref(false);

			const activePersonas = computed(() =>
				personas.value.filter((p) => !p.archived_at),
			);
			const visiblePersonas = computed(() =>
				showArchived.value
					? personas.value
					: personas.value.filter((p) => !p.archived_at),
			);

			const load = async () => {
				loading.value = true;
				loadError.value = null;
				try {
					personas.value = await listPersonas(true);
				} catch {
					loadError.value = 'Could not load personas.';
				} finally {
					loading.value = false;
				}
			};

			const toggleShowArchived = async (value: boolean) => {
				showArchived.value = value;
				await load();
			};

			const startEdit = (persona: Persona) => {
				editingId.value = persona.id;
				form.value = {
					name: persona.name,
					description: persona.description ?? '',
					system_prompt: persona.system_prompt ?? '',
				};
				formErrors.value = {};
			};

			const cancelEdit = () => {
				editingId.value = null;
				form.value = emptyForm();
				formErrors.value = {};
			};

			const submitForm = async () => {
				saving.value = true;
				formErrors.value = {};
				const payload: PersonaInput = {
					name: form.value.name,
					description: form.value.description || undefined,
					system_prompt: form.value.system_prompt || undefined,
				};
				try {
					if (editingId.value) {
						await updatePersona(editingId.value, payload);
						toast({ title: 'Persona updated' });
					} else {
						await createPersona(payload);
						toast({ title: 'Persona created' });
					}
					cancelEdit();
					await load();
				} catch (error) {
					const fieldErrors = extractFieldErrors(error);
					if (fieldErrors) {
						formErrors.value = fieldErrors.errors;
					} else {
						toast({
							title: 'Could not save persona',
							variant: 'destructive',
						});
					}
				} finally {
					saving.value = false;
				}
			};

			const archive = async (persona: Persona) => {
				try {
					await archivePersonaAction(persona.id);
					await load();
				} catch {
					toast({ title: 'Could not archive persona', variant: 'destructive' });
				}
			};

			const restore = async (persona: Persona) => {
				try {
					await restorePersonaAction(persona.id);
					await load();
				} catch {
					toast({
						title: `Could not restore persona (limit is ${PERSONA_LIMIT})`,
						variant: 'destructive',
					});
				}
			};

			const onAvatarChange = async (persona: Persona, event: Event) => {
				const file = (event.target as HTMLInputElement).files?.[0];
				if (!file) return;
				try {
					await uploadPersonaAvatar(persona.id, file);
					forgetPersonaAvatar(persona.id);
					await load();
				} catch {
					toast({ title: 'Could not upload avatar', variant: 'destructive' });
				} finally {
					(event.target as HTMLInputElement).value = '';
				}
			};

			onMounted(load);

			return {
				personas,
				loading,
				loadError,
				showArchived,
				toggleShowArchived,
				editingId,
				form,
				formErrors,
				saving,
				activePersonas,
				visiblePersonas,
				startEdit,
				cancelEdit,
				submitForm,
				archive,
				restore,
				onAvatarChange,
				PERSONA_LIMIT,
			};
		},
	});
</script>
