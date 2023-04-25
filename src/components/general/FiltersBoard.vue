<template>
	<div class="w-full pl-4">
		<div class="w-full flex items-center justify-between mr-20 rounded">
			<div class="flex items-center">
				<input
					class="cursor-pointer w-4 h-4 focus:outline-none rounded-lg"
					type="checkbox"
					id="checkbox"
					@change="$emit('handleUpdateDraggable', $event.target.checked)"
				/>
				<label class="ml-2 text-sm" for="checkbox">Reorder statuses</label>
			</div>
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
	const emit = defineEmits(['update:chosenUser', 'handleUpdateDraggable']);

	const props = defineProps({
		workspaceUsers: Array,
		chosenUser: Object,
		activeDraggable: Boolean,
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
