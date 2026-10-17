<template>
	<div
		ref="root"
		class="tour-sheet"
		:class="{ 'tour-sheet--collapsed': collapsed }"
		role="dialog"
		data-state="open"
		:aria-modal="modal ? 'true' : 'false'"
		:aria-labelledby="titleId"
		:aria-describedby="collapsed ? undefined : bodyId"
		tabindex="-1"
	>
		<template v-if="collapsed">
			<span :id="titleId" class="tour-sheet__title-line">{{ title }}</span>
			<button
				v-if="!waiting"
				type="button"
				class="tour-btn tour-btn--primary"
				@click="primary"
			>
				{{ primaryLabel }}
			</button>
			<button
				v-else
				type="button"
				class="tour-link"
				@click="$emit('skip-step')"
			>
				Skip step
			</button>
		</template>
		<template v-else>
			<span class="tour-sheet__handle" aria-hidden="true" />
			<div class="tour-pop__eyebrow">{{ eyebrow }}</div>
			<h2 :id="titleId" class="tour-pop__title">{{ title }}</h2>
			<p :id="bodyId" class="tour-pop__body">{{ body }}</p>
			<div v-if="total > 1" class="tour-sheet__progress" aria-hidden="true">
				<span :style="{ width: `${(position / total) * 100}%` }" />
			</div>
			<div v-if="waiting" class="tour-pop__waiting">
				<span class="tour-spinner" aria-hidden="true" />
				<span>{{ waitingText }}</span>
			</div>
			<button
				v-else
				type="button"
				class="tour-btn tour-btn--primary"
				@click="primary"
			>
				{{ primaryLabel }}
			</button>
			<div class="tour-sheet__row">
				<button type="button" class="tour-link" @click="$emit('skip-step')">
					Skip step
				</button>
				<button type="button" class="tour-link" @click="$emit('skip-tour')">
					Skip tour
				</button>
				<TourSkipMenu
					label="More"
					:keys="['pause', 'explore', 'never']"
					@select="onSelect"
				/>
			</div>
		</template>
	</div>
</template>

<script lang="ts">
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
	} from 'vue';
	import { isKeyboardOpen } from '../a11y';
	import TourSkipMenu, { type SkipMenuKey } from './TourSkipMenu.vue';

	let counter = 0;

	export default defineComponent({
		name: 'BottomSheetStep',
		components: { TourSkipMenu },
		props: {
			tourTitle: { type: String, default: '' },
			title: { type: String, required: true },
			body: { type: String, required: true },
			position: { type: Number, required: true },
			total: { type: Number, required: true },
			waiting: { type: Boolean, default: false },
			modal: { type: Boolean, default: true },
			canTakeMe: { type: Boolean, default: false },
			unavailable: { type: Boolean, default: false },
			waitingText: { type: String, default: 'Waiting for you…' },
		},
		emits: [
			'next',
			'take-me',
			'skip-step',
			'skip-tour',
			'pause',
			'explore',
			'never',
		],
		setup(props, { emit }) {
			counter += 1;
			const titleId = `tour-sheet-title-${counter}`;
			const bodyId = `tour-sheet-body-${counter}`;
			const collapsed = ref(false);
			const eyebrow = computed(() =>
				[props.tourTitle, `${props.position} of ${props.total}`]
					.filter(Boolean)
					.join(' · '),
			);

			function onViewport() {
				const vv = window.visualViewport;
				if (!vv) return;
				collapsed.value = isKeyboardOpen(vv.height, window.innerHeight);
				const bottom = Math.max(
					0,
					window.innerHeight - (vv.offsetTop + vv.height),
				);
				document.documentElement.style.setProperty(
					'--tour-vv-bottom',
					`${bottom}px`,
				);
			}

			onMounted(() => {
				window.visualViewport?.addEventListener('resize', onViewport);
				window.visualViewport?.addEventListener('scroll', onViewport);
				onViewport();
			});

			onBeforeUnmount(() => {
				window.visualViewport?.removeEventListener('resize', onViewport);
				window.visualViewport?.removeEventListener('scroll', onViewport);
				document.documentElement.style.removeProperty('--tour-vv-bottom');
			});

			function onSelect(key: SkipMenuKey) {
				emit(key);
			}

			const primaryLabel = computed(() =>
				props.canTakeMe
					? 'Take me there'
					: props.unavailable
					? 'Skip step'
					: props.position >= props.total
					? 'Done'
					: 'Next',
			);

			function primary() {
				emit(
					props.canTakeMe
						? 'take-me'
						: props.unavailable
						? 'skip-step'
						: 'next',
				);
			}

			return {
				titleId,
				bodyId,
				collapsed,
				eyebrow,
				onSelect,
				primaryLabel,
				primary,
			};
		},
	});
</script>
