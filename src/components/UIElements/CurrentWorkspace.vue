<template>
	<input-field
		v-if="workspaces.length > 0"
		v-model="val"
		type="select"
		:options="workspaces"
		option-name-key="name"
		option-value-key="id"
		selected
	></input-field>
</template>

<script>
	import InputField from "components/UIElements/InputField";
	export default {
		name: "CurrentWorkspace",
		components: {InputField},
		props: {
			modelValue: {
				required: false
			}
		},
		emits: [
			'update:modelValue'
		],
		data() {
			return {
				workspaces: []
			}
		},
		computed: {
			val: {
				get() {
					return parseInt(this.modelValue);
				},
				set(v) {
					return this.$emit('update:modelValue', v);
				}
			}
		},
		methods: {},
		async created() {
			let {data: {data: workspaces}} = await this.$axios.get('/workspaces');
			console.log(workspaces);
			this.workspaces = workspaces;
		}
	}
</script>

<style scoped lang="scss"></style>
