<template>
<modal
	:modal-width="width"
	:is-center="isCenter"
	:close-on-bg-click="cancelOnBgClick"
	@close="$emit('onCancel')">
	<template #modal-body>
		<slot name="title">
			<h3 :class="`${$color('textModal')} p-5`">
				<slot name="title-content">
					{{ title }}
				</slot>
			</h3>
		</slot>
		<slot name="body">
			<p :class="`${$color('textModal')} p-5`">
				<slot name="body-content">
					{{ body }}
				</slot>
			</p>
		</slot>
		<slot name="footer">
			<div :class="`${$color('textModal')} float-right`">
				<new-button color="green" @click="$emit('onOk')" class="pr-2">Ok</new-button>
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
	width?: Number,
	isCenter?: Boolean,
	cancelOnBgClick?: Boolean,
}

interface Context {
	attrs: Props,
	emit: Function
}

interface Data extends Props{
}

export default {
	name: "Confirm",
	emits: ['onOk', 'onCancel'],
	props: {
		title: String,
		body: String,
		width: {
			type: Number,
			default: 500
		},
		isCenter: {
			type: Boolean,
			default: true
		},
		cancelOnBgClick: {
			type: Boolean,
			default: true
		}
	},
	setup ({title, body, width = 500, isCenter = true, cancelOnBgClick = true}: Props, context: Context): Data {
		return {
			width,
			isCenter,
			cancelOnBgClick,
			title,
			body
		}
	}
}
</script>

<style scoped>

</style>
