<template>
	<div class="input_wrapper">
		<transition name="fade">
			<div v-if="showInput">
				<div v-if="type === 'select'" :class="`shadow appearance-none border rounded w-full ${extraClass || $color('input')} ${$color('borderMain')}`">
					<vue-select
						v-if="options"
						:label="optionNameKey"
						:options="preparedOptions"
						v-model="val"
					/>
				</div>
				<textarea
					:class="`shadow appearance-none border rounded w-full py-2 px-3 ${extraClass || $color('input')} ${$color('borderMain')}  leading-tight focus:outline-none focus:shadow-outline`"
					v-else-if="type === 'textarea'" name="" v-model="val"/>
				<input
					v-else-if="type === 'time_in_seconds'"
					:id="name"
					type="time"
					:class="`shadow ${$color('borderMain')} ${extraClass || $color('input')} appearance-none border rounded w-full py-2 px-3  leading-tight focus:outline-none focus:shadow-outline ${errors ? 'with-errors' : ''}`"
					:name="name"
					:placeholder="placeholder"
					v-model="val"
				/>
				<div
					v-else-if="type === 'checkbox'"
					class="b-switch-list mt-3"
				>
					<div
						class="b-switch-list__item"
					>
						<label class="b-switch">
							<input type="checkbox" :name="name" v-model="val">
							<span></span>
						</label>
						<div class="b-switch-list__text">
							<div class="b-switch-list__title" :class="$color('settingsTextColor')">{{ placeholder }}</div>
						</div>
					</div>
				</div>
				<input
					v-else
					:id="name"
					:type="type"
					:class="`shadow ${$color('borderMain')} ${extraClass || $color('input')} appearance-none border rounded w-full py-2 px-3  leading-tight focus:outline-none focus:shadow-outline ${errors ? 'with-errors' : ''}`"
					:name="name"
					:placeholder="placeholder"
					v-model="val"
				/>
				<transition name="fade-left">
					<div v-if="errors" class="error" :class="{ 'tooltip': errorAsTooltip }">
						{{ errors[0] }}
					</div>
				</transition>
			</div>
			<div v-else>
				{{ modelValue }} (<a href="#" @click.prevent="showInput = true">edit</a>)
			</div>
		</transition>
	</div>
</template>

<script>
	import getTimeInSeconds from './InputField/getTimeInSeconds'
	import toHHMM from './InputField/toHHMM'

	export default {
		name: "InputField",
		props: {
			modelValue: {
				required: false
			},
			errors: {
				required: false,
				type: Array,
				default: null
			},
			type: {
				required: false,
				type: String,
				default: 'text'
			},
			options: {
				required: false,
				type: Array
			},
			optionValueKey: {
				required: false,
				type: String,
				default: 'value'
			},
			optionNameKey: {
				required: false,
				type: String,
				default: 'name'
			},
			placeholder: {
				required: false,
				type: String,
				default: ''
			},
			name: {
				required: false,
				type: String
			},
			selected: {
				required: false,
				type: Boolean,
				default: false
			},
			extraClass: {
				required: false,
				type: String
			}
		},
		data() {
			return {
				screenWidth: null,
				showInput: true
			}
		},
		computed: {
				val: {
				get() {
					if (this.type === 'select') {
						return this.findOptionByValue(this.modelValue)
					}
					if (this.type !== 'time_in_seconds') {
						return this.modelValue
					}
					return this.getSecondsInTime(this.modelValue)
				},
				set(v) {
					if (this.type === 'select') {
						return this.$emit('update:modelValue', v[this.optionValueKey])
					}
					if (this.type !== 'time_in_seconds') {
						return this.$emit('update:modelValue', v)
					}
					this.$emit('update:modelValue', this.getTimeInSeconds(v))
				}
			},
			preparedOptions: {
				get() {
					const preparedOptions = []
					this.options.forEach(option => {
						option.label = option[this.optionNameKey]
						option.value = option[this.optionValueKey]
						preparedOptions.push(option)
					})
					return preparedOptions
				}
			},
			errorAsTooltip() {
				return this.screenWidth > 767
			}
		},
		methods: {
			findOptionByValue(value) {
				return this.options.find(option => option[this.optionValueKey] === value)
			},
			getTimeInSeconds,
			getSecondsInTime: toHHMM,
			updateWidth() {
				this.screenWidth = window.innerWidth;
			},
		},
		created() {
			this.updateWidth()
			window.addEventListener('resize', this.updateWidth);
		}
	}
</script>

<style scoped lang="scss">
input.with-errors {
	border: red solid 1px;
}

.error {
	font-size: .9em;
	color: red;

	&.tooltip {
		position: absolute;
		left: 108%;
		white-space: nowrap;
		background: #fff;
		padding: 0.75rem;
		min-width: 220px;
		display: inline-block;
		top: -6px;
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

.fade-left-enter-active, .fade-left-leave-active {
	transition: .5s;
	transform: translateX(0);
}

.fade-left-enter, .fade-left-leave-to {
	transform: translateX(25px);
}
</style>
