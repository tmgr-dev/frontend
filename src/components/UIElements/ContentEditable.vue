<template>
	<p
		ref="editable"
		class="content-editable text-left"
		:placeholder="placeholder"
		contenteditable
		v-on="listeners"
		v-html="modelValue"
	></p>
</template>

<script>
export default {
	name: 'ContentEditable',
	props: {
		modelValue: {
			type: String,
			default: '',
		},
		placeholder: {
			type: String,
			default: '',
		},
	},
	emits: [
		'input'
	],
	computed: {
		listeners() {
			return { ...this.$listeners, input: this.onInput };
		},
	},
	mounted() {
		this.$refs.editable.innerText = this.modelValue;
	},
	methods: {
		onInput(e) {
			this.$emit('update:modelValue', e.target.innerText);
		},
	},
};
</script>
<style scoped lang="scss">
 	.content-editable[placeholder]:empty::before {
		content: attr(placeholder);
		color: #9ba3af;
	}
</style>
