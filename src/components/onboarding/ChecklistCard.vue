<template>
	<div v-if="visible" ref="root" data-testid="checklist-card">
		<button
			v-if="hidden"
			type="button"
			class="min-h-[44px] text-sm text-ink-subtle underline hover:text-ink sm:min-h-0"
			data-testid="checklist-show"
			:disabled="busy"
			@click="setHidden(false)"
		>
			Show checklist
		</button>
		<div
			v-else
			class="rounded-card border border-line bg-surface p-4"
			role="region"
			aria-labelledby="checklist-title"
		>
			<div class="flex items-center justify-between gap-3">
				<p id="checklist-title" class="text-sm font-semibold text-ink">
					Getting started
					<span
						class="font-normal text-ink-subtle"
						data-testid="checklist-count"
					>
						{{ checklist.done }} of {{ checklist.total }}
					</span>
				</p>
				<div class="relative">
					<button
						type="button"
						class="flex h-11 w-11 items-center justify-center rounded text-ink-subtle hover:bg-surface-hover hover:text-ink sm:h-8 sm:w-8"
						aria-label="Checklist options"
						aria-haspopup="menu"
						:aria-expanded="menuOpen"
						data-testid="checklist-menu"
						@click="menuOpen = !menuOpen"
						@keydown.esc="menuOpen = false"
					>
						<span class="material-icons text-xl" aria-hidden="true"
							>more_horiz</span
						>
					</button>
					<div
						v-if="menuOpen"
						role="menu"
						@keydown.esc="menuOpen = false"
						class="absolute right-0 z-10 mt-1 w-44 rounded-md border border-line bg-surface py-1 shadow-lg"
					>
						<button
							type="button"
							role="menuitem"
							class="block min-h-[44px] w-full px-3 text-left text-sm text-ink hover:bg-surface-hover focus-visible:bg-surface-hover sm:min-h-[32px]"
							data-testid="checklist-hide"
							:disabled="busy"
							@click="setHidden(true)"
						>
							Hide checklist
						</button>
					</div>
				</div>
			</div>
			<div
				class="mt-2 h-2 w-full overflow-hidden rounded-full bg-surface-sunken"
				role="progressbar"
				aria-labelledby="checklist-title"
				aria-valuemin="0"
				:aria-valuemax="checklist.total"
				:aria-valuenow="checklist.done"
			>
				<div
					class="h-full rounded-full bg-brand transition-all"
					:style="{ width: progressWidth }"
				/>
			</div>
			<ul class="divide-y divide-line mt-2 flex flex-col">
				<li
					v-for="item in checklist.items"
					:key="item.id"
					class="flex min-h-[44px] items-center gap-3 py-2 text-sm sm:min-h-[36px]"
					:data-checklist-item="item.id"
					:data-done="item.done ? 'true' : 'false'"
				>
					<span
						class="material-icons text-xl"
						:class="item.done ? 'text-brand' : 'text-ink-subtle'"
						aria-hidden="true"
					>
						{{ item.done ? 'check_box' : 'check_box_outline_blank' }}
					</span>
					<span :class="item.done ? 'text-ink-muted' : 'text-ink'">
						{{ item.label }}
						<span v-if="item.optional" class="text-ink-subtle">(optional)</span>
						<em v-if="item.disabledMessage" class="text-ink-subtle">
							({{ item.disabledMessage }})
						</em>
					</span>
					<span v-if="item.done" class="sr-only">Done</span>
				</li>
			</ul>
		</div>
	</div>
</template>

<script lang="ts">
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import { buildChecklist } from '@/onboarding/checklist';
	import { useTours } from '@/tours/useTours';
	import { computed, defineComponent, onBeforeUnmount, ref, watch } from 'vue';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'ChecklistCard',
		setup() {
			const store = useStore();
			const { isFeatureEnabled } = useFeatureToggles();
			const tours = useTours();
			const busy = ref(false);
			const menuOpen = ref(false);
			const root = ref<HTMLElement | null>(null);

			const visible = computed(() => {
				if (store.getters.isLocalOnly) return false;
				const intent = tours.onboarding.value?.intent;
				return intent === 'human' || intent === 'agent';
			});

			const onDocumentPointer = (event: Event) => {
				if (!root.value?.contains(event.target as Node)) menuOpen.value = false;
			};
			const onDocumentKey = (event: KeyboardEvent) => {
				if (event.key === 'Escape') menuOpen.value = false;
			};
			const stopListening = () => {
				document.removeEventListener('pointerdown', onDocumentPointer);
				document.removeEventListener('keydown', onDocumentKey);
			};
			watch(menuOpen, (open) => {
				stopListening();
				if (!open) return;
				document.addEventListener('pointerdown', onDocumentPointer);
				document.addEventListener('keydown', onDocumentKey);
			});
			onBeforeUnmount(stopListening);

			const checklist = computed(() =>
				buildChecklist(
					tours.onboarding.value?.intent ?? null,
					tours.onboarding.value?.facts ?? {},
					{
						isModuleEnabled: (key: string) => isFeatureEnabled(key),
						modulesKnown: () => !!store.getters['featureToggles/modulesKnown'],
						canManageModules:
							store.getters['featureToggles/canManageModules'] === true,
						account: store.getters.isLocalOnly ? 'local' : 'cloud',
						emailVerified: !!(
							store.state.user as
								{ email_verified_at?: string | null } | undefined
						)?.email_verified_at,
					},
				),
			);
			const progressWidth = computed(() =>
				checklist.value.total
					? `${Math.round(
							(checklist.value.done / checklist.value.total) * 100,
						)}%`
					: '0%',
			);
			const hidden = computed(
				() => tours.onboarding.value?.checklist_hidden === true,
			);

			const setHidden = async (value: boolean) => {
				menuOpen.value = false;
				busy.value = true;
				try {
					await tours.setChecklistHidden(value);
				} catch (error) {
					console.error(error);
				} finally {
					busy.value = false;
				}
			};

			return {
				root,
				visible,
				checklist,
				progressWidth,
				hidden,
				busy,
				menuOpen,
				setHidden,
			};
		},
	});
</script>
