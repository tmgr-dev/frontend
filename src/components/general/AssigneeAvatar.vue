<template>
	<div class="m-auto font-sans text-base text-white" >
		{{name.charAt(0).toUpperCase()}}
	</div>

</template>

<script>
export default {
	name: "AssigneeAvatar",
	props: {

		name: name
	},}
</script>

<style scoped></style>
