<template>
	<div class="m-auto font-sans text-base text-white" >
		{{assignee?.name.charAt(0).toUpperCase()}}
	</div>
	<div v-if="isActiveDeleteAsign"
		class="invisible absolute -top-1.5 -right-1.5 flex h-4 w-4 cursor-pointer rounded-full bg-red-500 opacity-75 hover:opacity-100 group-hover:visible"
	>
					<span
						class="material-icons m-auto text-xs text-white"
						@click="deleteAssignee(assignee.id)"
					>
						close
					</span>
	</div>

</template>

<script>
export default {
	name: "AssigneeAvatar",
	props: {
		assignee:{
			type: Object,
			required:true
		},
		isActiveDeleteAsign:{
			type:Boolean,
			required: true,
		}
	},
	methods: {
		deleteAssignee(id){
			this.$emit('deleteAssign',id)
		}
	},}
</script>

<style scoped></style>
