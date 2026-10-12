<template>
	<Teleport v-if="hostEl && active" :to="hostEl">
		<component
			:is="sheet ? 'BottomSheetStep' : 'TourPopover'"
			:tour-title="active.tour.title"
			:title="titleText"
			:body="bodyText"
			:position="active.index + 1"
			:total="active.total"
			:waiting="active.step.type === 'wait'"
			:modal="presentation.mode !== 'passive'"
			@next="api.next()"
			@skip-step="api.skipStep()"
			@skip-tour="api.skipTour()"
			@pause="api.pause()"
			@explore="onExplore"
			@never="onNever"
		/>
	</Teleport>
	<Teleport to="body">
		<div v-if="pill" class="tour-pill-wrap" data-tour-resume>
			<button type="button" class="tour-pill" @click="resumePaused">
				Resume tour
			</button>
			<button
				type="button"
				class="tour-pill tour-pill--icon"
				aria-label="Dismiss"
				@click="pillDismissed = true"
			>
				&times;
			</button>
		</div>
	</Teleport>
</template>

<script lang="ts">
	import {
		computed,
		defineComponent,
		nextTick,
		onBeforeUnmount,
		ref,
		shallowRef,
		watch,
		type PropType,
	} from 'vue';
	import {
		announce,
		focusAnchor,
		inertAround,
		isSheetViewport,
		onEscape,
		rememberFocus,
		removeLiveRegion,
		SHEET_QUERY,
		stepAnnouncement,
		trapFocus,
	} from '../a11y';
	import { createOverlayAdapter } from '../overlay';
	import { watchAnchorEl } from '../overlay/anchorEl';
	import { useTours, type ToursApi } from '../useTours';
	import BottomSheetStep from './BottomSheetStep.vue';
	import TourPopover from './TourPopover.vue';
	import './tours.css';

	type Presentation = {
		mode: 'modal' | 'passive' | 'centered';
		anchorEl: Element | null;
	};

	export default defineComponent({
		name: 'TourHost',
		components: { BottomSheetStep, TourPopover },
		props: {
			api: { type: Object as PropType<ToursApi>, default: undefined },
		},
		setup(props) {
			const api = props.api ?? useTours();
			const active = api.active;
			const adapter = createOverlayAdapter();
			const hostEl = shallowRef<HTMLElement | null>(null);
			const presentation = ref<Presentation>({
				mode: 'centered',
				anchorEl: null,
			});
			const sheet = ref(isSheetViewport());
			const lastTourId = ref<string | null>(null);
			const pillDismissed = ref(false);

			let stopAnchor: (() => void) | null = null;
			let releaseA11y: Array<() => void> = [];
			let restoreFocus: (() => void) | null = null;
			let offEsc: (() => void) | null = null;

			const media =
				typeof window.matchMedia === 'function'
					? window.matchMedia(SHEET_QUERY)
					: null;
			const onMedia = () => {
				sheet.value = media?.matches ?? false;
			};
			media?.addEventListener('change', onMedia);

			const titleText = computed(() => active.value?.step.title ?? '');
			const bodyText = computed(() => {
				const a = active.value;
				if (!a) return '';
				return sheet.value && a.step.mobileBody
					? a.step.mobileBody
					: a.step.body;
			});

			const pausedItem = computed(() => {
				const id = lastTourId.value;
				if (!id || active.value) return null;
				const item = api.hubItems.value.find((i) => i.tour.id === id);
				return item?.state?.status === 'paused' ? item : null;
			});
			const pill = computed(() => !!pausedItem.value && !pillDismissed.value);

			function endStep() {
				stopAnchor?.();
				stopAnchor = null;
				adapter.hide();
				releaseA11y.forEach((fn) => fn());
				releaseA11y = [];
				hostEl.value = null;
			}

			function beginTour() {
				restoreFocus = rememberFocus();
				offEsc = onEscape(() => api.pause());
				pillDismissed.value = false;
			}

			function endTour() {
				offEsc?.();
				offEsc = null;
				removeLiveRegion();
				const restore = restoreFocus;
				restoreFocus = null;
				if (restore) void nextTick(restore);
			}

			function present(anchorEl: Element | null) {
				const a = active.value;
				if (!a) return;
				const wait = a.step.type === 'wait';
				const mode: Presentation['mode'] = !anchorEl
					? 'centered'
					: wait
					? 'passive'
					: 'modal';
				presentation.value = { mode, anchorEl };
				adapter.show({
					anchorEl,
					mode,
					mount: (host) => {
						hostEl.value = host;
						return () => {
							if (hostEl.value === host) hostEl.value = null;
						};
					},
				});
			}

			let hadTour = false;
			watch(
				() => {
					const a = active.value;
					return a
						? `${a.tour.id}:${a.step.id}:${sheet.value ? 'm' : 'd'}`
						: null;
				},
				(key) => {
					endStep();
					if (!key) {
						if (hadTour) endTour();
						hadTour = false;
						return;
					}
					const a = active.value;
					if (!a) return;
					if (!hadTour) beginTour();
					hadTour = true;
					lastTourId.value = a.tour.id;
					const name =
						sheet.value && a.step.mobileAnchor
							? a.step.mobileAnchor
							: a.step.anchor;
					stopAnchor = watchAnchorEl(name, present);
				},
				{ immediate: true },
			);

			watch(hostEl, async (host) => {
				releaseA11y.forEach((fn) => fn());
				releaseA11y = [];
				if (!host) return;
				await nextTick();
				const dialog = host.querySelector<HTMLElement>('[role="dialog"]');
				const { mode, anchorEl } = presentation.value;
				if (mode === 'passive') {
					if (!anchorEl?.contains(document.activeElement)) {
						focusAnchor(anchorEl);
					}
				} else if (dialog) {
					const decor = Array.from(
						document.querySelectorAll(
							'.driver-overlay,.tour-backdrop,.tour-ring',
						),
					);
					releaseA11y.push(inertAround([host, anchorEl, ...decor]));
					releaseA11y.push(trapFocus(dialog, dialog));
				}
				announceStep();
			});

			function announceStep() {
				const a = active.value;
				if (!a) return;
				announce(
					stepAnnouncement({
						position: a.index + 1,
						total: a.total,
						title: a.step.title,
						waiting: a.step.type === 'wait',
					}),
				);
			}

			function onExplore() {
				api.pause();
				void api.setTipsMode('minimal');
			}

			function onNever() {
				api.pause();
				void api.setTipsMode('off');
			}

			function resumePaused() {
				const item = pausedItem.value;
				if (item) void api.resume(item.tour.id);
			}

			onBeforeUnmount(() => {
				media?.removeEventListener('change', onMedia);
				endStep();
				endTour();
			});

			return {
				api,
				active,
				hostEl,
				presentation,
				sheet,
				titleText,
				bodyText,
				pill,
				pillDismissed,
				onExplore,
				onNever,
				resumePaused,
			};
		},
	});
</script>
