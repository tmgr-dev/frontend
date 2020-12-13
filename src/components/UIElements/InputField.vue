<template>
	<div class="input_wrapper">
		<transition name="fade">
			<div v-if="showInput">
				<select
					v-if="type === 'select'"
					name=""
					:class="`block appearance-none w-full  ${$color('input')} ${$color('borderMain')} border px-4 py-2 pr-8 rounded shadow leading-tight focus:outline-none focus:shadow-outline ${selected ? 'text-gray-500' : ''}`"
					v-model="val"
					:disabled="!options || !options.length"
				>
					<option v-for="option in options" :key="option[optionValueKey]" :value="option[optionValueKey]">
						{{ option[optionNameKey] }}
					</option>
				</select>
				<textarea
					:class="`shadow appearance-none border rounded w-full py-2 px-3 ${$color('input')} ${$color('borderMain')}  leading-tight focus:outline-none focus:shadow-outline`"
					v-else-if="type === 'textarea'" name="" v-model="val">
                </textarea>
				<input
					v-else
					:id="name"
					:type="type"
					:class="`shadow ${$color('borderMain')} ${extraClass || $color('input')} appearance-none border rounded w-full py-2 px-3  leading-tight focus:outline-none focus:shadow-outline ${errors ? 'with-errors' : ''}`"
					:name="name"
					:placeholder="placeholder"
					v-model="val"
				>
				<transition name="fade-left">
					<div v-if="errors" class="error" :class="{ 'tooltip': errorAsTooltip }">
						{{ errors[0] }}
					</div>
				</transition>
			</div>
			<div v-else>
				{{ value }} (<a href="#" @click.prevent="showInput = true">edit</a>)
			</div>
		</transition>
	</div>
</template>

<script>
	export default {
		name: "InputField",
		props: {
			value: {
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
					return this.value
				},
				set(v) {
					this.$emit('update:value', v)
				}
			},
			errorAsTooltip() {
				return this.screenWidth > 767
			}
		},
		methods: {
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
