<template>
	<div class="flex flex-col gap-3" data-testid="persona-shared-skills">
		<h5 class="text-xs font-semibold">From library</h5>
		<p v-if="loading" class="text-xs text-muted-foreground">
			Loading library skills…
		</p>

		<div v-else class="flex flex-col gap-2">
			<p v-if="attached.length === 0" class="text-xs text-muted-foreground">
				No library skills attached. Create one in the Skills library below the
				personas, then attach it here.
			</p>
			<div
				v-for="skill in attached"
				:key="skill.slug"
				class="flex items-center justify-between gap-3 rounded border border-border p-2"
			>
				<div class="min-w-0">
					<p class="truncate text-sm font-medium">{{ skill.title }}</p>
					<p class="truncate text-xs text-muted-foreground">
						{{ skill.slug }} · v{{ skill.version }} · {{ skill.when }}
					</p>
					<span
						v-if="skill.overridden"
						data-testid="shared-skill-overridden"
						class="mt-1 inline-block rounded bg-amber-100 px-1.5 py-0.5 text-2xs font-semibold text-amber-800 dark:bg-amber-900/30 dark:text-amber-300"
					>
						Overridden by this persona's own skill
					</span>
				</div>
				<Button variant="outline" size="sm" @click="detach(skill)">
					Detach
				</Button>
			</div>
		</div>

		<p class="text-2xs text-muted-foreground">
			{{ attached.length }}/{{ ATTACHED_SKILL_LIMIT }} attached
		</p>

		<div v-if="!loading && attachable.length > 0">
			<Select :model-value="''" @update:model-value="(v) => attach(String(v))">
				<SelectTrigger aria-label="Attach a library skill">
					<SelectValue placeholder="Attach a library skill" />
				</SelectTrigger>
				<SelectContent>
					<SelectItem
						v-for="skill in attachable"
						:key="skill.slug"
						:value="skill.slug"
					>
						{{ skill.title }} ({{ skill.slug }})
					</SelectItem>
				</SelectContent>
			</Select>
		</div>
	</div>
</template>

<script lang="ts">
	import {
		attachSharedSkill,
		detachSharedSkill,
		listPersonaSharedSkills,
		listSharedSkills,
		type Persona,
		type PersonaSharedSkill,
		type SharedSkillSummary,
	} from '@/actions/tmgr/personas';
	import { Button } from '@/components/ui/button';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue,
	} from '@/components/ui/select';
	import { toast } from '@/components/ui/toast';
	import {
		ATTACHED_SKILL_LIMIT,
		attachableSharedSkills,
		extractFieldErrors,
	} from '@/utils/personas';
	import {
		computed,
		defineComponent,
		onMounted,
		ref,
		watch,
		type PropType,
	} from 'vue';

	export default defineComponent({
		name: 'PersonaSharedSkills',
		components: {
			Button,
			Select,
			SelectContent,
			SelectItem,
			SelectTrigger,
			SelectValue,
		},
		props: {
			persona: { type: Object as PropType<Persona>, required: true },
			revision: { type: Number, default: 0 },
		},
		emits: ['changed'],
		setup(props, { emit }) {
			const attached = ref<PersonaSharedSkill[]>([]);
			const library = ref<SharedSkillSummary[]>([]);
			const loading = ref(true);

			const load = async () => {
				loading.value = true;
				try {
					[attached.value, library.value] = await Promise.all([
						listPersonaSharedSkills(props.persona.id),
						listSharedSkills(),
					]);
				} catch {
					toast({
						title: 'Could not load library skills',
						variant: 'destructive',
					});
				} finally {
					loading.value = false;
				}
			};

			const attachable = computed(() =>
				attachableSharedSkills(library.value, attached.value),
			);

			const fail = (error: unknown, fallback: string) => {
				const fieldErrors = extractFieldErrors(error);
				const detail = fieldErrors
					? Object.values(fieldErrors.errors).flat().join(' ')
					: '';
				toast({
					title: fieldErrors ? fieldErrors.message : fallback,
					description: detail || undefined,
					variant: 'destructive',
				});
			};

			const attach = async (slug: string) => {
				if (!slug) return;
				try {
					await attachSharedSkill(props.persona.id, slug);
					await load();
					emit('changed');
				} catch (error) {
					fail(error, 'Could not attach skill');
				}
			};

			const detach = async (skill: PersonaSharedSkill) => {
				try {
					await detachSharedSkill(props.persona.id, skill.slug);
					await load();
					emit('changed');
				} catch (error) {
					fail(error, 'Could not detach skill');
				}
			};

			watch(() => props.revision, load);
			onMounted(load);

			return {
				attached,
				attachable,
				loading,
				attach,
				detach,
				ATTACHED_SKILL_LIMIT,
			};
		},
	});
</script>
