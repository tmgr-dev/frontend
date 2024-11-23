<template>
	<div class="relative">
		<input
			type="time"
			class="w-full appearance-none rounded border-0 bg-white px-3 py-2 outline-none transition-colors duration-300 dark:bg-gray-800"
			:class="[extraClass, errors ? 'with-errors' : '']"
			:name="name"
			:placeholder="placeholder"
			v-model="val"
		/>

		<transition name="fade-left">
			<div v-if="hasError" class="error" :class="{ tooltip: errorAsTooltip }">
				{{ errors[0] }}
			</div>
		</transition>
	</div>
</template>

<script>
	import getTimeInSeconds from '@/utils/getTimeInSeconds';
	import { convertToHHMM } from '@/utils/timeUtils.js';

	// @todo refactor this component. It was just moved from input-field

	export default {
		name: 'TimeField',
		props: {
			modelValue: {
				required: false,
				default: null,
			},
			errors: {
				required: false,
				type: Array,
				default: null,
			},
			placeholder: {
				required: false,
				type: String,
				default: '',
			},
			name: {
				required: false,
				type: String,
			},
			extraClass: {
				required: false,
				type: String,
			},
		},
		emits: ['keydown:enter', 'update:modelValue'],
		data() {
			return {
				screenWidth: null,
				value: null,
				updateKey: 0,
				showInput: true,
			};
		},
		computed: {
			hasError() {
				return this.errors?.length > 0 && this.errors[0];
			},
			val: {
				get() {
					return this.getSecondsInTime(this.modelValue);
				},
				set(v) {
					this.$emit('update:modelValue', this.getTimeInSeconds(v));
				},
			},
			errorAsTooltip() {
				return this.screenWidth > 767;
			},
		},
		methods: {
			getTimeInSeconds,
			getSecondsInTime: convertToHHMM,
			updateWidth() {
				this.screenWidth = window.innerWidth;
			},
		},
		created() {
			this.updateWidth();
			window.addEventListener('resize', this.updateWidth);
		},
	};
</script>

<style scoped lang="scss">
	input.with-errors {
		border: red solid 1px;
	}

	.error {
		font-size: 0.9em;
		color: red;

		&.tooltip {
			position: absolute;
			left: 108%;
			background: #fff;
			padding: 0.75rem;
			min-width: 220px;
			display: inline-block;
			top: 50%;
			transform: translateY(-50%);
			border-radius: 7px;
			box-shadow: 1px 1px 4px #888;
			line-height: 1.1;

			&:before {
				content: '';
				border: 6px solid transparent;
				border-right-color: #ffffff;
				position: absolute;
				left: -12px;
				z-index: 2;
				top: calc(50% - 5px);
			}

			&:after {
				content: '';
				border: 8px solid transparent;
				border-right-color: #d0d0d0;
				position: absolute;
				top: calc(50% - 7px);
				left: -16px;
			}
		}
	}
</style>
