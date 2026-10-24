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
	import { apiErrorMessage } from '@/local/moveToCloud/api';
	import { formatBytes, previousMoveWarning } from '@/local/moveToCloud/banner';
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
			const sizeLabel = computed(() =>
				dry.value ? formatBytes(dry.value.movedFileBytes) : '',
			);
			const excludedSummary = computed(() => {
				const excluded = dry.value?.excluded ?? [];
				if (!excluded.length) return '';
				const reasons = [
					...new Set(excluded.map((f) => reasonLabel(f.reason))),
				];
				return `${excluded.length} ${
					excluded.length === 1 ? 'file' : 'files'
				} that can't be uploaded (${reasons.join(', ')})`;
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
				<DialogTitle>Move to your cloud account</DialogTitle>
				<DialogDescription>
					Copies the local workspace <strong>{{ name }}</strong> into a new
					workspace in your TMGR account.
				</DialogDescription>
			</DialogHeader>

			<p v-if="loading" class="text-sm text-ink-subtle">
				Checking this workspace…
			</p>
			<p v-if="error" role="alert" class="text-sm text-destructive">
				{{ error }}
			</p>

			<div
				v-if="dry"
				class="flex max-h-[60vh] flex-col gap-4 overflow-y-auto pr-1"
			>
				<label class="flex items-center gap-3 text-sm">
					<span class="shrink-0">Cloud workspace name:</span>
					<input
						v-model="cloudName"
						data-selectable
						maxlength="120"
						class="h-9 min-w-0 flex-1 rounded-md border border-input bg-background px-3 text-sm outline-hidden"
					/>
				</label>

				<div class="grid gap-4 sm:grid-cols-2">
					<div>
						<h3 class="mb-2 text-sm font-semibold">Moves</h3>
						<ul class="flex flex-col gap-1.5 text-sm">
							<li class="flex gap-2">
								<Check class="mt-0.5 size-4 shrink-0 text-emerald-500" />
								<span
									><strong>{{ counts?.tasks }}</strong> tasks with time and
									checkpoints</span
								>
							</li>
							<li class="flex gap-2">
								<Check class="mt-0.5 size-4 shrink-0 text-emerald-500" />
								<span
									><strong>{{ counts?.comments }}</strong> comments</span
								>
							</li>
							<li class="flex gap-2">
								<Check class="mt-0.5 size-4 shrink-0 text-emerald-500" />
								<span
									><strong>{{ dry.movedFiles }}</strong> files ({{
										sizeLabel
									}})</span
								>
							</li>
							<li class="flex gap-2">
								<Check class="mt-0.5 size-4 shrink-0 text-emerald-500" />
								<span
									><strong>{{ counts?.pages }}</strong> pages with history</span
								>
							</li>
							<li class="flex gap-2">
								<Check class="mt-0.5 size-4 shrink-0 text-emerald-500" />
								<span
									><strong>{{ counts?.routines }}</strong> routines</span
								>
							</li>
							<li class="flex gap-2">
								<Check class="mt-0.5 size-4 shrink-0 text-emerald-500" />
								<span
									><strong>{{ counts?.agentRuns }}</strong> agent runs</span
								>
							</li>
							<li class="flex gap-2">
								<Check class="mt-0.5 size-4 shrink-0 text-emerald-500" />
								<span>Statuses, categories, relations</span>
							</li>
						</ul>
					</div>
					<div>
						<h3 class="mb-2 text-sm font-semibold">Stays on this computer</h3>
						<ul class="flex flex-col gap-1.5 text-sm">
							<li class="flex gap-2">
								<Minus class="mt-0.5 size-4 shrink-0 text-ink-subtle" />
								<span>Persona tokens</span>
							</li>
							<li class="flex gap-2">
								<Minus class="mt-0.5 size-4 shrink-0 text-ink-subtle" />
								<span>Plugin data</span>
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
										{{ showExcluded ? 'Hide' : 'Show' }}
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
					<h3 class="mb-2 text-sm font-semibold">Personas</h3>
					<ul class="flex flex-col gap-2 text-sm">
						<li
							v-for="choice in localChoices"
							:key="choice.uuid"
							class="flex flex-wrap items-center gap-2"
						>
							<span class="min-w-0 flex-1 truncate">{{ choice.name }}</span>
							<select
								:value="choice.action"
								:aria-label="`What to do with ${choice.name}`"
								class="h-9 rounded-md border border-input bg-background px-2 text-sm"
								@change="
									setAction(choice, ($event.target as HTMLSelectElement).value)
								"
							>
								<option value="create">Create in my account</option>
								<option value="map" :disabled="!cloudPersonas.length">
									Use my existing persona …
								</option>
								<option value="me">Attribute to me</option>
							</select>
							<select
								v-if="choice.action === 'map'"
								v-model="choice.cloudUuid"
								:aria-label="`Existing persona for ${choice.name}`"
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
							<span>Uses the same persona in your account</span>
						</li>
					</ul>
				</div>

				<label class="flex items-center gap-2 border-t pt-3 text-sm">
					<input type="checkbox" checked disabled />
					Keep the local copy
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
							Tasks, pages, comments and files from this workspace will be
							uploaded to your TMGR account.
						</p>
						<p v-if="dry.runningTimers > 0">
							A timer is running and will be stopped first.
						</p>
					</div>
				</div>
			</div>

			<DialogFooter>
				<Button variant="outline" @click="cancel">Cancel</Button>
				<Button variant="destructive" :disabled="!valid" @click="start">
					Move to cloud
				</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>
