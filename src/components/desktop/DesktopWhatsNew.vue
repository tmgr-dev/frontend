<template>
	<Dialog :open="state.open" @update:open="onOpenChange">
		<DialogContent
			class="max-w-lg gap-0 p-0"
			@close-auto-focus="onCloseAutoFocus"
			@escape-key-down="onEscapeKeyDown"
		>
			<div class="flex max-h-[85vh] flex-col" data-testid="whats-new-modal">
				<DialogHeader class="border-b border-border px-6 py-4 pr-12">
					<DialogTitle class="text-base">{{ title }}</DialogTitle>
					<DialogDescription class="sr-only">
						Release notes for this version.
					</DialogDescription>
				</DialogHeader>

				<div class="max-h-[70vh] space-y-6 overflow-y-auto px-6 py-4">
					<section v-for="section in view.sections" :key="section.version">
						<div class="mb-2 flex items-baseline gap-2">
							<h3 class="text-sm font-semibold">v{{ section.version }}</h3>
							<span v-if="section.date" class="text-xs text-muted-foreground">
								{{ section.date }}
							</span>
						</div>
						<MarkdownText :content="section.body" :link-task-keys="false" />
						<div
							v-if="toursFor(section.version).length"
							class="mt-3 flex flex-wrap gap-2"
						>
							<Button
								v-for="item in toursFor(section.version)"
								:key="item.id"
								type="button"
								variant="outline"
								size="sm"
								data-tour="whats-new.item-tour"
								@click="takeTour(item.id)"
							>
								Take the tour: {{ item.title }}
							</Button>
						</div>
					</section>
					<p
						v-if="view.sections.length === 0"
						class="text-sm text-muted-foreground"
					>
						No release notes for this version.
					</p>
					<PromotionsList
						v-if="promotionsEnabled"
						placement="release_notes"
						heading="By the way, we made…"
						compact
						class="border-t border-border pt-4"
					/>
				</div>

				<DialogFooter class="border-t border-border px-6 py-3">
					<Button type="button" @click="close">Got it</Button>
				</DialogFooter>
			</div>
		</DialogContent>
	</Dialog>
</template>

<script>
	import MarkdownText from '@/components/general/MarkdownText.vue';
	import PromotionsList from '@/components/promotions/PromotionsList.vue';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import router from '@/router';
	import store from '@/store';
	import { useTours } from '@/tours/useTours';
	import { availableTours, WHATS_NEW_TOUR_ID } from '@/tours/whatsNewTours';
	import {
		checkWhatsNewOnStartup,
		closeWhatsNew,
		hadSessionAtLaunch,
		whatsNewState,
	} from '@/utils/whatsNewState';
	import { computed, defineComponent, onMounted, ref, watch } from 'vue';

	export default defineComponent({
		name: 'DesktopWhatsNew',
		components: {
			Button,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
			MarkdownText,
			PromotionsList,
		},
		setup() {
			const { isFeatureEnabled } = useFeatureToggles();
			let checked = false;
			const hasPriorData = hadSessionAtLaunch();

			const ready = () =>
				store.getters.canUseApp && router.currentRoute.value.path !== '/app';

			const check = () => {
				if (checked || !ready()) return;
				checked = true;
				checkWhatsNewOnStartup(hasPriorData);
			};

			onMounted(async () => {
				await router.isReady();
				watch(
					() => [store.getters.canUseApp, router.currentRoute.value.path],
					check,
					{ immediate: true },
				);
			});

			// closeWhatsNew empties sections at once; keep the last open state so the
			// exit animation doesn't flash the empty-state text.
			const view = ref(whatsNewState.value);
			watch(whatsNewState, (next) => {
				if (next.open) view.value = next;
			});

			const tours = useTours();
			const hubItem = (id) =>
				tours.hubItems.value.find((item) => item.tour.id === id);
			const startStatus = (id) => {
				const item = hubItem(id);
				if (!item || !item.available) return 'unavailable';
				return item.state?.status === 'completed' ? 'completed' : 'available';
			};
			const toursFor = (version) =>
				availableTours(version, startStatus).map((id) => ({
					id,
					title: hubItem(id).tour.title,
				}));
			let touring = false;
			const takeTour = (id) => {
				if (!whatsNewState.value.open) return;
				touring = true;
				closeWhatsNew();
				void tours.start(id, 'whats_new');
			};
			const onCloseAutoFocus = (event) => {
				if (!touring) return;
				touring = false;
				event.preventDefault();
			};
			// The dialog keeps focus inside itself, so the tour's own Esc handler
			// skips the key while its popover sits over the dialog.
			const onEscapeKeyDown = (event) => {
				if (tours.active.value?.tour.id !== WHATS_NEW_TOUR_ID) return;
				event.preventDefault();
				tours.pause();
			};
			const onOpenChange = (open) => {
				if (!open && whatsNewState.value.open) closeWhatsNew();
			};
			watch(
				() =>
					whatsNewState.value.open &&
					whatsNewState.value.sections.some(
						(section) => toursFor(section.version).length > 0,
					),
				(shown) => {
					const item = hubItem(WHATS_NEW_TOUR_ID);
					if (shown && item?.available && !item.state) {
						void tours.start(WHATS_NEW_TOUR_ID, 'whats_new');
					}
				},
				{ immediate: true },
			);

			return {
				toursFor,
				takeTour,
				onCloseAutoFocus,
				onEscapeKeyDown,
				onOpenChange,
				state: whatsNewState,
				view,
				promotionsEnabled: computed(() => isFeatureEnabled('promotions')),
				close: closeWhatsNew,
				title: computed(() =>
					view.value.version
						? `What's new in ${view.value.version}`
						: `What's new`,
				),
			};
		},
	});
</script>
