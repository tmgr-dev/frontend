<template>
	<Teleport v-if="hostEl && active" :to="hostEl">
		<component
			:is="sheet ? 'BottomSheetStep' : 'TourPopover'"
			:tour-title="tourTitleText"
			:title="titleText"
			:body="bodyText"
			:body-parts="bodyPartsView"
			:position="active.index + 1"
			:total="active.total"
			:waiting="active.waiting"
			:modal="modal"
			:can-take-me="active.canTakeMe === true"
			:unavailable="active.unavailable === true"
			@next="api.next()"
			@take-me="api.takeMeThere()"
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
				{{ $t('onboarding.tours.host.resume') }}
			</button>
			<button
				type="button"
				class="tour-pill tour-pill--icon"
				:aria-label="$t('onboarding.tours.host.dismiss')"
				@click="api.dismissPill()"
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
	import { t } from '@/i18n';
	import { localizedStep, tourTitle } from '../i18n';
	import { partsToText } from '../keycaps';
	import { createOverlayAdapter } from '../overlay';
	import type {
		OverlayAdapter,
		OverlayShowOptions,
	} from '../overlay/OverlayAdapter';
	import type { ActiveTour } from '../useTours';
	import { setOverlayAdapter, useTours, type ToursApi } from '../useTours';
	import BottomSheetStep from './BottomSheetStep.vue';
	import TourPopover from './TourPopover.vue';
	import './tours.css';

	type Presentation = Pick<
		OverlayShowOptions,
		'mode' | 'anchorEl' | 'blocking'
	>;

	export default defineComponent({
		name: 'TourHost',
		components: { BottomSheetStep, TourPopover },
		props: {
			api: { type: Object as PropType<ToursApi>, default: undefined },
		},
		setup(props) {
			const api = props.api ?? useTours();
			const active = api.active;
			const hostEl = shallowRef<HTMLElement | null>(null);
			const presentation = ref<Presentation>({
				mode: 'centered',
				anchorEl: null,
			});
			const sheet = ref(isSheetViewport());

			const inner = createOverlayAdapter();
			const adapter: OverlayAdapter = {
				show(options) {
					presentation.value = {
						mode: options.mode,
						anchorEl: options.anchorEl,
						blocking: options.blocking,
					};
					inner.show(options);
				},
				refresh: () => inner.refresh(),
				hide: () => inner.hide(),
			};
			setOverlayAdapter(adapter, (host) => {
				hostEl.value = host;
				return () => {
					if (hostEl.value === host) hostEl.value = null;
				};
			});

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

			const bodySuffix = (a: ActiveTour) =>
				a.unavailable && a.onTargetPage
					? ` ${t('onboarding.tours.host.hints.notOpen')}`
					: a.unavailable
						? ` ${t('onboarding.tours.host.hints.notFound')}`
						: a.hint === 'timer_too_short'
							? ` ${t('onboarding.tours.host.hints.timerTooShort')}`
							: '';
			const tourTitleText = computed(() =>
				active.value ? tourTitle(active.value.tour) : '',
			);
			const step = computed(() =>
				active.value
					? localizedStep(active.value.tour, active.value.step)
					: null,
			);
			const parts = computed(() =>
				active.value
					? api.bodyPartsFor(active.value.tour, active.value.step)
					: undefined,
			);
			const titleText = computed(() => step.value?.title ?? '');
			const bodyText = computed(() => {
				const a = active.value;
				if (!a) return '';
				const s = step.value;
				if (!s) return '';
				const base =
					sheet.value && s.mobileBody
						? s.mobileBody
						: parts.value
							? partsToText(parts.value)
							: s.body;
				return base + bodySuffix(a);
			});
			const bodyPartsView = computed(() => {
				const a = active.value;
				const bodyParts = parts.value;
				if (!a || !bodyParts || (sheet.value && step.value?.mobileBody))
					return null;
				const suffix = bodySuffix(a);
				return suffix
					? [...bodyParts, { kind: 'text' as const, text: suffix }]
					: bodyParts;
			});

			const modal = computed(
				() =>
					presentation.value.mode !== 'passive' &&
					presentation.value.blocking !== false,
			);
			const pill = computed(() => api.resumePill.value);

			watch(hostEl, async (host) => {
				releaseA11y.forEach((fn) => fn());
				releaseA11y = [];
				if (!host) return;
				await nextTick();
				if (hostEl.value !== host) return;
				const dialog = host.querySelector<HTMLElement>('[role="dialog"]');
				const { mode, anchorEl } = presentation.value;
				if (!modal.value) {
					if (
						mode === 'passive' &&
						!anchorEl?.contains(document.activeElement)
					) {
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

			function beginTour() {
				restoreFocus = rememberFocus();
				offEsc = onEscape(() => api.pause(), {
					swallow: () => modal.value,
					ownDialog: () =>
						hostEl.value?.querySelector('[role="dialog"]') ?? null,
				});
			}

			function endTour() {
				offEsc?.();
				offEsc = null;
				releaseA11y.forEach((fn) => fn());
				releaseA11y = [];
				hostEl.value = null;
				removeLiveRegion();
				const restore = restoreFocus;
				restoreFocus = null;
				if (restore) void nextTick(restore);
			}

			watch(
				() => active.value?.tour.id ?? null,
				(id, before) => {
					if (id && !before) beginTour();
					else if (!id && before) endTour();
				},
				{ immediate: true },
			);

			function announceStep() {
				const a = active.value;
				if (!a) return;
				announce(
					stepAnnouncement({
						position: a.index + 1,
						total: a.total,
						title: step.value?.title ?? a.step.title,
						waiting: a.waiting,
					}),
				);
			}

			function onExplore() {
				api.pause();
				void api.setTipsMode('hub_only');
			}

			function onNever() {
				api.pause();
				void api.setTipsMode('off');
			}

			function resumePaused() {
				if (pill.value) void api.resume(pill.value.tourId);
			}

			onBeforeUnmount(() => {
				media?.removeEventListener('change', onMedia);
				setOverlayAdapter(null);
				inner.hide();
				if (offEsc) endTour();
			});

			return {
				api,
				active,
				hostEl,
				presentation,
				sheet,
				titleText,
				tourTitleText,
				bodyText,
				bodyPartsView,
				pill,
				modal,
				onExplore,
				onNever,
				resumePaused,
			};
		},
	});
</script>
