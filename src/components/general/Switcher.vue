<template>
	<div class="b-switch-list mt-3">
		<div class="b-switch-list__item">
			<label class="b-switch">
				<input
					type="checkbox"
					:name="name"
					v-model="value"
					@keydown:enter="$emit('keydown:enter', value)"
				/>
				<span />
			</label>

			<div class="b-switch-list__text">
				<div class="b-switch-list__title text-gray-800 dark:text-gray-400">
					{{ placeholder }}
				</div>
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
	interface Props {
		name: string;
		modelValue: string;
		placeholder: string;
	}

	const props = defineProps<Props>();
	const value = defineModel();
</script>

<style lang="scss" scoped>
	$base-text-color: #3a4564;
	$blue-text-color: #0066cc;
	$gray-text-color: #8e94a6;
	$red-text-color: #d55f5f;

	.b-switch-list__item {
		display: flex;
		margin-bottom: 20px;
	}

	.b-switch-list__item:last-child {
		margin-bottom: 0;
	}

	.b-switch-list__item .b-switch {
		flex-shrink: 0;
		margin-right: 20px;
	}

	.b-switch-list__title {
		font-weight: 500;
		font-size: 16px;
		line-height: 20px;
		margin-top: 4px;
	}

	.b-switch-list__teaser {
		font-size: 14px;
		color: $gray-text-color;
		line-height: 20px;
	}

	.b-switch {
		display: inline-block;
		position: relative;
		cursor: pointer;
		-webkit-user-select: none;
		-moz-user-select: none;
		-ms-user-select: none;
		user-select: none;
		-webkit-tap-highlight-color: transparent;
		tap-highlight-color: transparent;
		font-size: 20px;
	}

	.b-switch input {
		opacity: 0;
		position: absolute;
	}

	.b-switch input + span {
		position: relative;
		display: inline-block;
		width: 2em;
		height: 1em;
		background: #a1a8bc;
		border-radius: 0.5em;
		vertical-align: -0.15em;
		transition: all 0.4s cubic-bezier(0.17, 0.67, 0.43, 0.98);
	}

	.b-switch input + span:after {
		position: absolute;
		display: block;
		content: '';
		width: 0.8em;
		height: 0.8em;
		border-radius: 50%;
		top: 2px;
		left: 2px;
		background: white;
		transition: all 0.25s ease-out;
	}

	.b-switch input:checked + span {
		box-shadow: inset 0 0 0 0.73em #6fd554;
	}

	.b-switch input:checked + span:after {
		left: 1.1em;
	}

	/* accessibility styles */
	.b-switch input:focus + span:after {
		box-shadow: inset 0 0 0 0.03em rgba(0, 0, 0, 0.15),
			0 0 0.05em rgba(0, 0, 0, 0.08), 0 0.1em 0.2em rgba(0, 0, 0, 0.3);
		background: #fff;
	}

	.b-switch input:focus + span {
		box-shadow: inset 0 0 0 0.0625em #dadada;
	}

	.b-switch input:focus:checked + span {
		box-shadow: inset 0 0 0 0.73em #6fd554;
	}

	/* reset accessibility style on hover */
	.b-switch:hover input:focus + span:after {
		box-shadow: inset 0 0 0 0.03em rgba(0, 0, 0, 0.1),
			0 0 0.05em rgba(0, 0, 0, 0.05), 0 0.1em 0.2em rgba(0, 0, 0, 0.2);
		background: #fff;
	}

	.b-switch:hover input:focus + span {
		box-shadow: inset 0 0 0 0.0625em #e9e9e9;
	}

	.b-switch:hover input:focus:checked + span {
		box-shadow: inset 0 0 0 0.73em #6fd554;
	}
</style>
