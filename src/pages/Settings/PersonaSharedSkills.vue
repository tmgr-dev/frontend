<template>
	<div class="flex flex-col gap-3" data-testid="persona-shared-skills">
		<h5 class="text-xs font-semibold">
			{{ $t('settingsPages.personaSharedSkills.fromLibrary') }}
		</h5>
		<p v-if="loading" class="text-xs text-muted-foreground">
			{{ $t('settingsPages.personaSharedSkills.loading') }}
		</p>

		<div v-else class="flex flex-col gap-2">
			<p v-if="attached.length === 0" class="text-xs text-muted-foreground">
				{{ $t('settingsPages.personaSharedSkills.empty') }}
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
						v-if="skill.pinned_version"
						data-testid="shared-skill-pinned"
						class="mt-1 mr-1 inline-block rounded bg-blue-100 px-1.5 py-0.5 text-2xs font-semibold text-blue-800 dark:bg-blue-900/30 dark:text-blue-300"
					>
						{{
							$t('settingsPages.personaSharedSkills.pinned', {
								pinned: skill.pinned_version,
								latest: skill.latest_version,
							})
						}}
					</span>
					<span
						v-if="skill.pinned_version && skill.files_count"
						data-testid="shared-skill-files-not-pinned"
						class="mt-1 mr-1 inline-block text-2xs text-muted-foreground"
					>
						{{ $t('settingsPages.personaSharedSkills.filesNotPinned') }}
					</span>
					<span
						v-if="skill.overridden"
						data-testid="shared-skill-overridden"
						class="mt-1 inline-block rounded bg-amber-100 px-1.5 py-0.5 text-2xs font-semibold text-amber-800 dark:bg-amber-900/30 dark:text-amber-300"
					>
						{{ $t('settingsPages.personaSharedSkills.overridden') }}
					</span>
				</div>
				<div class="flex shrink-0 items-center gap-2">
					<Select
						:model-value="
							skill.pinned_version ? String(skill.pinned_version) : 'latest'
						"
						:disabled="pendingSlugs.has(skill.slug)"
						@update:model-value="(v) => setVersion(skill, String(v))"
						@update:open="(open) => open && loadVersions(skill)"
					>
						<SelectTrigger
							class="h-8 w-40 text-xs"
							:aria-label="
								$t('settingsPages.personaSharedSkills.versionFor', {
									slug: skill.slug,
								})
							"
						>
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							<SelectItem value="latest">
								{{
									$t('settingsPages.personaSharedSkills.latest', {
										version: skill.latest_version,
									})
								}}
							</SelectItem>
							<SelectItem
								v-for="v in versionsFor(skill)"
								:key="v"
								:value="String(v)"
							>
								v{{ v }}
							</SelectItem>
						</SelectContent>
					</Select>
					<Button variant="outline" size="sm" @click="detach(skill)">
						{{ $t('settingsPages.personaSharedSkills.detach') }}
					</Button>
				</div>
			</div>
		</div>

		<p class="text-2xs text-muted-foreground">
			{{
				$t('settingsPages.personaSharedSkills.attachedCount', {
					count: attached.length,
					limit: ATTACHED_SKILL_LIMIT,
				})
			}}
		</p>

		<div v-if="!loading && attachable.length > 0">
			<Select :model-value="''" @update:model-value="(v) => attach(String(v))">
				<SelectTrigger
					:aria-label="$t('settingsPages.personaSharedSkills.attach')"
				>
					<SelectValue
						:placeholder="$t('settingsPages.personaSharedSkills.attach')"
					/>
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
		listSharedSkillVersions,
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
	import { changePin } from '@/composable/sharedSkillVersions';
	import { toast } from '@/components/ui/toast';
	import { t } from '@/i18n';
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
			const keptVersions = ref<Record<string, number[]>>({});

			const staleSlugs = new Set<string>();
			const pendingSlugs = ref<Set<string>>(new Set());
			const versionRequests = new Map<string, number>();

			const loadVersions = async (skill: PersonaSharedSkill) => {
				if (keptVersions.value[skill.slug] && !staleSlugs.has(skill.slug))
					return;
				const requestId = (versionRequests.get(skill.slug) ?? 0) + 1;
				versionRequests.set(skill.slug, requestId);
				try {
					const rows = await listSharedSkillVersions(skill.slug, false);
					if (versionRequests.get(skill.slug) !== requestId) return;
					keptVersions.value = {
						...keptVersions.value,
						[skill.slug]: rows.map((r) => r.version),
					};
					staleSlugs.delete(skill.slug);
				} catch {
					if (versionRequests.get(skill.slug) !== requestId) return;
					toast({
						title: t('settingsPages.personaSharedSkills.versionsFailed'),
						variant: 'destructive',
					});
				}
			};

			const versionsFor = (skill: PersonaSharedSkill) => {
				const kept = keptVersions.value[skill.slug] ?? [];
				return skill.pinned_version && !kept.includes(skill.pinned_version)
					? [skill.pinned_version, ...kept]
					: kept;
			};

			const load = async () => {
				loading.value = true;
				try {
					[attached.value, library.value] = await Promise.all([
						listPersonaSharedSkills(props.persona.id),
						listSharedSkills(),
					]);
				} catch {
					toast({
						title: t('settingsPages.personaSharedSkills.loadFailed'),
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
					fail(error, t('settingsPages.personaSharedSkills.attachFailed'));
				}
			};

			const detach = async (skill: PersonaSharedSkill) => {
				try {
					await detachSharedSkill(props.persona.id, skill.slug);
					await load();
					emit('changed');
				} catch (error) {
					fail(error, t('settingsPages.personaSharedSkills.detachFailed'));
				}
			};

			const setVersion = async (skill: PersonaSharedSkill, choice: string) => {
				const current = skill.pinned_version
					? String(skill.pinned_version)
					: 'latest';
				if (choice === current || pendingSlugs.value.has(skill.slug)) return;
				pendingSlugs.value = new Set(pendingSlugs.value).add(skill.slug);
				try {
					await changePin(props.persona.id, skill.slug, choice);
					await load();
					emit('changed');
				} catch (error) {
					fail(error, t('settingsPages.personaSharedSkills.versionFailed'));
				} finally {
					staleSlugs.add(skill.slug);
					const next = new Set(pendingSlugs.value);
					next.delete(skill.slug);
					pendingSlugs.value = next;
				}
			};

			watch(
				() => props.revision,
				() => {
					keptVersions.value = {};
					staleSlugs.clear();
					versionRequests.forEach((id, slug) =>
						versionRequests.set(slug, id + 1),
					);
					load();
				},
			);
			onMounted(load);

			return {
				attached,
				attachable,
				loading,
				attach,
				detach,
				setVersion,
				pendingSlugs,
				loadVersions,
				versionsFor,
				ATTACHED_SKILL_LIMIT,
			};
		},
	});
</script>
