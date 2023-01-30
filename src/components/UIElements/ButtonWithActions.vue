<template>
	<div
		:class="`inline-block relative font-bold text-white shadow-md rounded hover:rounded focus:outline-none bg-${color}-500`">
		<button
			:class="`px-4 ${buttonClass}`"
			type="button"
			@click.prevent="() => {actions[defaultActionIndex].action(); showDropdownButtons=false;}"
		>
			{{ actions[defaultActionIndex].label }}
		</button>
		<button
			v-tooltip.right="userSettings.showTooltips
						? 'Save & ...'
						: { visible: false }
			"
			:class="`${buttonClass} hover:rounded-r`"
			type="button"
			@click.prevent="showDropdownButtons = !showDropdownButtons"
		>
			<span v-if="!showDropdownButtons" class="material-icons">keyboard_arrow_{{ direction }}</span>
			<span v-else class="material-icons">keyboard_arrow_{{ direction === 'up' ? 'down' : 'up' }}</span>
		</button>
		<div :class="classesForDirection[direction]" class="" :style="{top: `${dropdownOffset}px`}" v-if="showDropdownButtons" ref="dropdown">
			<template
				v-for="(action, i) in actions"
			>
				<button
					:class="`w-full px-4 ${buttonClass}`"
					type="button"
					v-if="i !== defaultActionIndex"
					@click.prevent="() => {action.action(); showDropdownButtons=false;}"
				>
					{{ action.label }}
				</button>
			</template>
		</div>
	</div>
</template>

<script>
	export default {
		name: 'ButtonWithActions',
		props: {
			defaultActionIndex: {
				required: false,
				type: Number,
				default: 0
			},
			direction: {
				required: false,
				type: String,
				default: 'down'
			},
			buttonClass: {
				required: false,
				type: String,
				default: 'hover:bg-blue-600'
			},
			actions: {
				required: true,
				type: Array
			},
			classesForDirection: {
				required: false,
				type: String,
				default: () => ({
					down: `absolute z-50 w-full bg-blue-700`,
					up: `absolute z-50 w-full bg-blue-700`
				})
			},
		},
		computed: {
			userSettings() {
				return this.$store.getters.getUserSettings ?? {};
			},
		},
		watch: {
			showDropdownButtons() {
				this.$nextTick(() => {
					this.dropdownOffset = this.direction === 'up' ? -this.$refs.dropdown.clientHeight : 0;
				})
			}
		},
		data () {
			return {
				showDropdownButtons: false,
				dropdownOffset: 0
			}
		}
	};
</script>

<style lang="scss" scoped>
	button {
		&:hover,
		&:focus,
		&:active {
			outline: none;
		}

		min-height: 45px;
		line-height: 1;
	}
	.hover\:rounded-tl-lg {
		border-top-left-radius: 0.5rem; /* 8px */
	}
	.hover\:rounded-tr-lg {
		border-top-right-radius: 0.5rem; /* 8px */
	}
	.hover\:rounded-bl-lg {
		border-bottom-left-radius: 0.5rem; /* 8px */
	}
	.hover\:rounded-br-lg {
		border-bottom-right-radius: 0.5rem; /* 8px */
	}
	.hover\:rounded-l-lg {
		border-top-left-radius: 0.5rem; /* 8px */
		border-bottom-left-radius: 0.5rem; /* 8px */
	}
	.hover\:rounded-r-lg {
		border-top-right-radius: 0.5rem; /* 8px */
		border-bottom-right-radius: 0.5rem; /* 8px */
	}
</style>
