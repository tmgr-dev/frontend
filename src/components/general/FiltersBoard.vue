<template>
	<div class="w-full pl-4">
		<div class="w-full flex justify-end mr-20 rounded">
			<div class="flex">
				<div class="w-48 py-2">
					<Select
						placeholder="Select user"
						:options="workspaceUsers"
						v-model="selectedUser"
						label-key="name"
						value-key="id"
					/>
				</div>
			</div>
		</div>
	</div>
</template>

<script setup>
	import { onMounted, ref, watch } from 'vue';
	import { useStore } from 'vuex';
	import { defineEmits } from 'vue';
	import Select from 'src/components/general/Select.vue';
	const emit = defineEmits(['update:chosenUser']);

	const props = defineProps({
		workspaceUsers: Array,
		chosenUser: Object,
	});

	const store = useStore();
	const selectedUser = ref(0);
	const userOptions = props.workspaceUsers;
	const tasks = ref([]);

	watch(selectedUser, () => {
		emit(
			'update:chosenUser',
			userOptions.find((option) => option.id === selectedUser.value),
		);
	});
</script>
