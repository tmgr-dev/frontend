<template>
	<div
		:class="`relative font-bold text-white shadow-md rounded hover:rounded focus:outline-none ${buttonClass} w-full`">
		<button
			:class="`px-4 h-full`"
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
			:class="`h-full opacity-50 hover:opacity-100`"
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
			wrapperClass: {
				required: false,
				type: String,
				default: 'bg-blue-500'
			},
			buttonClass: {
				required: false,
				type: String,
				default: 'bg-blue-500 hover:bg-blue-600'
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
</style>
