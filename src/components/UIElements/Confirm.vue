<template>
	<modal
		:close-on-bg-click="cancelOnBgClick"
		modal-class="p-6"
		@close="$emit('onCancel')"
	>
		<template #modal-body>
			<slot name="title">
				<div :class="`${$color('textModal')} text-main p-2 text-xl text-bold`">
					<slot name="title-content">
						{{ title }}
					</slot>
				</div>
			</slot>

			<slot name="body">
				<p :class="`${$color('textModal')} p-2`">
					<slot name="body-content">
						{{ body }}
					</slot>
				</p>
			</slot>

			<slot name="footer">
				<div :class="`${$color('textModal')} float-right`">
					<new-button class="pr-2" color="green" @click="$emit('onOk')">Ok</new-button>
					<new-button color="gray" @click="$emit('onCancel')">Cancel</new-button>
				</div>
			</slot>
		</template>
	</modal>
</template>

<script lang="ts">

interface Props {
	title: String,
	body: String,
	isCenter?: Boolean,
	cancelOnBgClick?: Boolean,
}

interface Context {
	attrs: Props,
	emit: Function
}

interface Data extends Props {
}

export default {
	name: 'Confirm',
	emits: ['onOk', 'onCancel'],
	props: {
		title: String,
		body: String,
		cancelOnBgClick: {
			type: Boolean,
			default: true
		}
	},
	setup({ title, body, width = 500, isCenter = true, cancelOnBgClick = true }: Props): Data {
		return {
			width,
			isCenter,
			cancelOnBgClick,
			title,
			body
		};
	}
};
</script>

<style scoped>

</style>
