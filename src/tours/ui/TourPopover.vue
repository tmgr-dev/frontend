<template>
	<div
		ref="root"
		class="tour-pop"
		role="dialog"
		data-state="open"
		:data-tour-step-type="waiting ? 'wait' : 'step'"
		:aria-modal="modal ? 'true' : 'false'"
		:aria-labelledby="titleId"
		:aria-describedby="bodyId"
		tabindex="-1"
	>
		<div class="tour-pop__eyebrow">{{ eyebrow }}</div>
		<h2 :id="titleId" class="tour-pop__title">{{ title }}</h2>
		<p
			:id="bodyId"
			ref="bodyEl"
			class="tour-pop__body"
			:tabindex="scrollable ? 0 : undefined"
		>
			<template v-if="bodyParts">
				<template v-for="(part, i) in bodyParts" :key="i">
					<kbd v-if="part.kind === 'key'" class="tour-kbd">{{ part.text }}</kbd>
					<template v-else>{{ part.text }}</template>
				</template>
			</template>
			<template v-else>{{ body }}</template>
		</p>
		<div v-if="total > 1" class="tour-pop__dots" aria-hidden="true">
			<span
				v-for="n in total"
				:key="n"
				class="tour-pop__dot"
				:data-active="n === position ? 'true' : 'false'"
			/>
		</div>
		<div v-if="waiting" class="tour-pop__waiting">
			<span class="tour-spinner" aria-hidden="true" />
			<span>{{ waitingText }}</span>
		</div>
		<div class="tour-pop__footer">
			<template v-if="waiting">
				<div class="tour-pop__links">
					<button type="button" class="tour-link" @click="$emit('skip-step')">
						Skip this step
					</button>
					<button type="button" class="tour-link" @click="$emit('skip-tour')">
						Skip tour
					</button>
				</div>
				<TourSkipMenu
					label="More"
					:keys="['pause', 'explore', 'never']"
					@select="onSelect"
				/>
			</template>
			<template v-else>
				<TourSkipMenu label="Skip" @select="onSelect" />
				<button
					type="button"
					class="tour-btn tour-btn--primary"
					@click="primary"
				>
					{{ primaryLabel }}
				</button>
			</template>
		</div>
	</div>
</template>

<script lang="ts">
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		type PropType,
	} from 'vue';
	import type { BodyPart } from '../keycaps';
	import TourSkipMenu, { type SkipMenuKey } from './TourSkipMenu.vue';

	let counter = 0;

	export default defineComponent({
		name: 'TourPopover',
		components: { TourSkipMenu },
		props: {
			tourTitle: { type: String, default: '' },
			title: { type: String, required: true },
			body: { type: String, required: true },
			bodyParts: { type: Array as PropType<BodyPart[] | null>, default: null },
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
			const titleId = `tour-title-${counter}`;
			const bodyId = `tour-body-${counter}`;
			const eyebrow = computed(() =>
				[props.tourTitle, `Step ${props.position} of ${props.total}`]
					.filter(Boolean)
					.join(' · '),
			);

			const bodyEl = ref<HTMLElement | null>(null);
			const scrollable = ref(false);
			const measure = () => {
				const el = bodyEl.value;
				scrollable.value = !!el && el.scrollHeight > el.clientHeight;
			};
			let resize: ResizeObserver | null = null;
			onMounted(() => {
				measure();
				if (typeof ResizeObserver === 'function' && bodyEl.value) {
					resize = new ResizeObserver(measure);
					resize.observe(bodyEl.value);
				}
				window.addEventListener('resize', measure);
			});
			onBeforeUnmount(() => {
				resize?.disconnect();
				window.removeEventListener('resize', measure);
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
				bodyEl,
				scrollable,
				eyebrow,
				onSelect,
				primaryLabel,
				primary,
			};
		},
	});
</script>
