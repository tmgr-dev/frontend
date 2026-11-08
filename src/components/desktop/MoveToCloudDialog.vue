<script lang="ts">
	import { listPersonas, type Persona } from '@/actions/tmgr/personas';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { t } from '@/i18n';
	import { apiErrorMessage } from '@/local/moveToCloud/api';
	import { formatBytes, previousMoveWarning } from '@/local/moveToCloud/banner';
	import { dryRunSkillsLine } from '@/local/moveToCloud/skills';
	import {
		toMappingPayload,
		type DryRun,
		type PersonaChoice,
	} from '@/local/moveToCloud/flow';
	import { reasonLabel } from '@/local/moveToCloud/model';
	import { getMover } from '@/local/moveToCloud/runtime';
	import { openMoveProgress } from '@/local/moveToCloud/ui';
	import { Check, Minus, TriangleAlert } from 'lucide-vue-next';
	import { computed, defineComponent, ref, watch } from 'vue';

	export default defineComponent({
		name: 'MoveToCloudDialog',
		components: {
			Button,
			Check,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
			Minus,
			TriangleAlert,
		},
		props: {
			open: { type: Boolean, default: false },
			code: { type: String, default: '' },
			name: { type: String, default: '' },
		},
		emits: ['update:open'],
		setup(props, { emit }) {
			const dry = ref<DryRun | null>(null);
			const loading = ref(false);
			const error = ref('');
			const cloudName = ref('');
			const choices = ref<PersonaChoice[]>([]);
			const cloudPersonas = ref<Persona[]>([]);
			const showExcluded = ref(false);

			const load = async () => {
				loading.value = true;
				error.value = '';
				dry.value = null;
				try {
					const [result, personas] = await Promise.all([
						getMover().dryRun(props.code, props.name),
						listPersonas().catch(() => [] as Persona[]),
					]);
					dry.value = result;
					choices.value = result.personas.map((p) => ({ ...p }));
					cloudPersonas.value = personas.filter((p) => !p.archived_at);
					cloudName.value = props.name;
					if (result.blocker) error.value = result.blocker;
				} catch (e) {
					error.value = apiErrorMessage(e);
				} finally {
					loading.value = false;
				}
			};

			watch(
				() => props.open,
				(isOpen) => {
					if (isOpen) void load();
				},
				{ immediate: true },
			);

			const localChoices = computed(() => choices.value.filter((c) => c.local));
			const cachedCloud = computed(() => choices.value.filter((c) => !c.local));

			const setAction = (choice: PersonaChoice, action: string) => {
				choice.action = action as PersonaChoice['action'];
				choice.cloudUuid =
					action === 'map' ? (cloudPersonas.value[0]?.id ?? null) : null;
			};

			const valid = computed(
				() =>
					!!dry.value &&
					!dry.value.blocker &&
					!!cloudName.value.trim() &&
					choices.value.every((c) => c.action !== 'map' || !!c.cloudUuid),
			);

			const counts = computed(() => dry.value?.inventory.counts);
			const skillsLine = computed(() =>
				dryRunSkillsLine(dry.value?.skills ?? 0),
			);
			const sizeLabel = computed(() =>
				dry.value ? formatBytes(dry.value.movedFileBytes) : '',
			);
			const excludedSummary = computed(() => {
				const excluded = dry.value?.excluded ?? [];
				if (!excluded.length) return '';
				const reasons = [
					...new Set(excluded.map((f) => reasonLabel(f.reason))),
				];
				return t(
					'desktop.moveDialog.excluded',
					{ n: excluded.length, reasons: reasons.join(', ') },
					excluded.length,
				);
			});
			const previousWarning = computed(() =>
				dry.value?.previousMove
					? previousMoveWarning(dry.value.previousMove)
					: '',
			);

			const start = () => {
				if (!dry.value || !valid.value) return;
				getMover()
					.start(props.code, props.name, dry.value, {
						cloudName: cloudName.value.trim(),
						personas: toMappingPayload(choices.value),
					})
					.catch((e) => {
						error.value = apiErrorMessage(e);
					});
				openMoveProgress(props.code);
				emit('update:open', false);
			};

			return {
				cancel: () => emit('update:open', false),
				cachedCloud,
				choices,
				cloudName,
				cloudPersonas,
				counts,
				skillsLine,
				dry,
				error,
				excludedSummary,
				loading,
				localChoices,
				previousWarning,
				setAction,
				showExcluded,
				sizeLabel,
				start,
				valid,
				reasonLabel,
			};
		},
	});
</script>

<template>
	<Dialog :open="open" @update:open="$emit('update:open', $event)">
		<DialogContent class="max-w-2xl" data-testid="move-to-cloud-dialog">
			<DialogHeader>
				<DialogTitle>{{ $t('desktop.moveDialog.title') }}</DialogTitle>
				<DialogDescription>
					<i18n-t keypath="desktop.moveDialog.description" tag="span">
						<template #name>
							<strong>{{ name }}</strong>
						</template>
					</i18n-t>
				</DialogDescription>
			</DialogHeader>

			<p v-if="loading" class="text-sm text-ink-subtle">
				{{ $t('desktop.moveDialog.checking') }}
			</p>
			<p v-if="error" role="alert" class="text-sm text-destructive">
				{{ error }}
			</p>

			<div
				v-if="dry"
				class="flex max-h-[60vh] flex-col gap-4 overflow-y-auto pr-1"
			>
				<label class="flex items-center gap-3 text-sm">
					<span class="shrink-0">{{ $t('desktop.moveDialog.cloudName') }}</span>
					<input
						v-model="cloudName"
						data-selectable
						maxlength="120"
						class="h-9 min-w-0 flex-1 rounded-md border border-input bg-background px-3 text-sm outline-hidden"
					/>
				</label>

				<div class="grid gap-4 sm:grid-cols-2">
					<div>
						<h3 class="mb-2 text-sm font-semibold">
							{{ $t('desktop.moveDialog.moves') }}
						</h3>
						<ul class="flex flex-col gap-1.5 text-sm">
							<li class="flex gap-2">
								<Check class="mt-0.5 size-4 shrink-0 text-emerald-500" />
								<i18n-t keypath="desktop.moveDialog.tasks" tag="span">
									<template #n
										><strong>{{ counts?.tasks }}</strong></template
									>
								</i18n-t>
							</li>
							<li class="flex gap-2">
								<Check class="mt-0.5 size-4 shrink-0 text-emerald-500" />
								<i18n-t keypath="desktop.moveDialog.comments" tag="span">
									<template #n
										><strong>{{ counts?.comments }}</strong></template
									>
								</i18n-t>
							</li>
							<li class="flex gap-2">
								<Check class="mt-0.5 size-4 shrink-0 text-emerald-500" />
								<i18n-t keypath="desktop.moveDialog.files" tag="span">
									<template #n
										><strong>{{ dry.movedFiles }}</strong></template
									>
									<template #size>{{ sizeLabel }}</template>
								</i18n-t>
							</li>
							<li class="flex gap-2">
								<Check class="mt-0.5 size-4 shrink-0 text-emerald-500" />
								<i18n-t keypath="desktop.moveDialog.pages" tag="span">
									<template #n
										><strong>{{ counts?.pages }}</strong></template
									>
								</i18n-t>
							</li>
							<li class="flex gap-2">
								<Check class="mt-0.5 size-4 shrink-0 text-emerald-500" />
								<i18n-t keypath="desktop.moveDialog.routines" tag="span">
									<template #n
										><strong>{{ counts?.routines }}</strong></template
									>
								</i18n-t>
							</li>
							<li class="flex gap-2">
								<Check class="mt-0.5 size-4 shrink-0 text-emerald-500" />
								<i18n-t keypath="desktop.moveDialog.agentRuns" tag="span">
									<template #n
										><strong>{{ counts?.agentRuns }}</strong></template
									>
								</i18n-t>
							</li>
							<li class="flex gap-2">
								<Check class="mt-0.5 size-4 shrink-0 text-emerald-500" />
								<span>{{ $t('desktop.moveDialog.structure') }}</span>
							</li>
							<li v-if="dry.skills > 0" class="flex gap-2">
								<Check class="mt-0.5 size-4 shrink-0 text-emerald-500" />
								<span>{{ skillsLine }}</span>
							</li>
						</ul>
					</div>
					<div>
						<h3 class="mb-2 text-sm font-semibold">
							{{ $t('desktop.moveDialog.stays') }}
						</h3>
						<ul class="flex flex-col gap-1.5 text-sm">
							<li class="flex gap-2">
								<Minus class="mt-0.5 size-4 shrink-0 text-ink-subtle" />
								<span>{{ $t('desktop.moveDialog.personaTokens') }}</span>
							</li>
							<li class="flex gap-2">
								<Minus class="mt-0.5 size-4 shrink-0 text-ink-subtle" />
								<span>{{ $t('desktop.moveDialog.pluginData') }}</span>
							</li>
							<li v-if="dry.excluded.length" class="flex gap-2">
								<Minus class="mt-0.5 size-4 shrink-0 text-ink-subtle" />
								<span>
									{{ excludedSummary }}
									<button
										type="button"
										class="ml-1 underline underline-offset-4"
										:aria-expanded="showExcluded"
										@click="showExcluded = !showExcluded"
									>
										{{
											showExcluded
												? $t('desktop.moveDialog.hide')
												: $t('desktop.moveDialog.show')
										}}
									</button>
									<ul
										v-if="showExcluded"
										class="mt-1 flex flex-col gap-0.5 text-xs text-ink-subtle"
									>
										<li
											v-for="file in dry.excluded"
											:key="file.id"
											class="break-all"
										>
											{{ file.name }} ({{ reasonLabel(file.reason) }})
										</li>
									</ul>
								</span>
							</li>
						</ul>
					</div>
				</div>

				<div v-if="choices.length" class="border-t pt-3">
					<h3 class="mb-2 text-sm font-semibold">
						{{ $t('desktop.moveDialog.personas') }}
					</h3>
					<ul class="flex flex-col gap-2 text-sm">
						<li
							v-for="choice in localChoices"
							:key="choice.uuid"
							class="flex flex-wrap items-center gap-2"
						>
							<span class="min-w-0 flex-1 truncate">{{ choice.name }}</span>
							<select
								:value="choice.action"
								:aria-label="
									$t('desktop.moveDialog.actionLabel', { name: choice.name })
								"
								class="h-9 rounded-md border border-input bg-background px-2 text-sm"
								@change="
									setAction(choice, ($event.target as HTMLSelectElement).value)
								"
							>
								<option value="create">
									{{ $t('desktop.moveDialog.actionCreate') }}
								</option>
								<option value="map" :disabled="!cloudPersonas.length">
									{{ $t('desktop.moveDialog.actionMap') }}
								</option>
								<option value="me">
									{{ $t('desktop.moveDialog.actionMe') }}
								</option>
							</select>
							<select
								v-if="choice.action === 'map'"
								v-model="choice.cloudUuid"
								:aria-label="
									$t('desktop.moveDialog.existingLabel', { name: choice.name })
								"
								class="h-9 rounded-md border border-input bg-background px-2 text-sm"
							>
								<option
									v-for="persona in cloudPersonas"
									:key="persona.id"
									:value="persona.id"
								>
									{{ persona.name }}
								</option>
							</select>
						</li>
						<li
							v-for="choice in cachedCloud"
							:key="choice.uuid"
							class="flex gap-2 text-ink-subtle"
						>
							<span class="min-w-0 flex-1 truncate">{{ choice.name }}</span>
							<span>{{ $t('desktop.moveDialog.samePersona') }}</span>
						</li>
					</ul>
				</div>

				<label class="flex items-center gap-2 border-t pt-3 text-sm">
					<input type="checkbox" checked disabled />
					{{ $t('desktop.moveDialog.keepLocal') }}
				</label>

				<p
					v-if="previousWarning"
					role="status"
					class="text-sm text-amber-600 dark:text-amber-400"
				>
					{{ previousWarning }}
				</p>

				<div class="flex gap-2 text-sm text-destructive" role="note">
					<TriangleAlert class="mt-0.5 size-4 shrink-0" />
					<div>
						<p>
							{{ $t('desktop.moveDialog.uploadWarning') }}
						</p>
						<p v-if="dry.runningTimers > 0">
							{{ $t('desktop.moveDialog.timerWarning') }}
						</p>
					</div>
				</div>
			</div>

			<DialogFooter>
				<Button variant="outline" @click="cancel">{{
					$t('common.actions.cancel')
				}}</Button>
				<Button variant="destructive" :disabled="!valid" @click="start">
					{{ $t('desktop.moveDialog.move') }}
				</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>
