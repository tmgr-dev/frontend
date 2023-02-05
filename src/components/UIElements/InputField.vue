<template>
	<div class="input_wrapper">
		<transition name="fade">
			<div>
				<div v-if="showInput" :key="updateKey">
					<input
						v-if="type === 'time_in_seconds'"
						:id="name"
						type="time"
						class="w-full appearance-none rounded border-0 bg-white py-2 px-3 leading-tight outline-none transition-colors duration-300 dark:bg-gray-800"
						:class="[extraClass, errors ? 'with-errors' : '']"
						:name="name"
						:placeholder="placeholder"
						v-model="val"
					/>

					<input
						v-else
						:id="name"
						:type="type"
						class="w-full appearance-none rounded border border-neutral-300 bg-white py-2 px-3 leading-tight outline-none transition-colors duration-300"
						:class="[
							extraClass,
							errors ? 'with-errors' : '',
							!isAuthPage && 'dark:border-transparent dark:bg-gray-800',
						]"
						:name="name"
						:placeholder="placeholder"
						v-model="val"
					/>

					<transition name="fade-left">
						<div
							v-if="hasError"
							class="error"
							:class="{ tooltip: errorAsTooltip }"
						>
							{{ errors[0] }}
						</div>
					</transition>
				</div>

				<div v-else>
					{{ modelValue }} (<a href="#" @click.prevent="showInput = true"
						>edit</a
					>)
				</div>
			</div>
		</transition>
	</div>
</template>

<script>
	import getTimeInSeconds from './InputField/getTimeInSeconds';
	import toHHMM from './InputField/toHHMM';
	import ContentEditable from './ContentEditable';

	export default {
		name: 'InputField',
		components: {
			ContentEditable,
		},
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
			type: {
				required: false,
				type: String,
				default: 'text',
			},
			options: {
				required: false,
				type: Array,
			},
			optionValueKey: {
				required: false,
				type: String,
				default: 'value',
			},
			optionNameKey: {
				required: false,
				type: String,
				default: 'name',
			},
			placeholder: {
				required: false,
				type: String,
				default: '',
			},
			forCheckpoint: {
				required: false,
				type: Boolean,
				default: false,
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
			isAuthPage() {
				// @todo it is temporary solution until this component refactored. Input shouldn't know about $route
				return ['Login', 'Register', 'ForgetPassword', 'NewPassword'].includes(
					this.$route.name,
				);
			},
			val: {
				get() {
					if (this.type !== 'time_in_seconds') {
						return this.modelValue;
					}

					return this.getSecondsInTime(this.modelValue);
				},
				set(v) {
					if (this.type !== 'time_in_seconds') {
						return this.$emit('update:modelValue', v);
					}

					this.$emit('update:modelValue', this.getTimeInSeconds(v));
				},
			},
			preparedOptions: {
				get() {
					const preparedOptions = [];
					this.options
						.filter((o) => !!o)
						.forEach((option) => {
							option.label = option[this.optionNameKey];
							option.value = option[this.optionValueKey];
							preparedOptions.push(option);
						});
					return preparedOptions;
				},
			},
			errorAsTooltip() {
				return this.screenWidth > 767;
			},
		},
		methods: {
			getTimeInSeconds,
			getSecondsInTime: toHHMM,
			updateWidth() {
				this.screenWidth = window.innerWidth;
			},
		},
		mounted() {
			if (this.type === 'contenteditable') {
				setTimeout(() => ++this.updateKey, 250);
			}
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

			div {
				margin-bottom: 5px;
			}

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

	.input_wrapper {
		position: relative;
	}

	.fade-left-enter-active,
	.fade-left-leave-active {
		transition: 0.5s;
		transform: translateX(0);
	}

	.fade-left-enter-from,
	.fade-left-leave-to {
		transform: translateX(25px);
	}
</style>
