<template>
	<div class="mt-10 px-4">
		<div
			class="w-full h-64 max-h-80 overflow-y-scroll dark:bg-gray-800 rounded p-4"
		>
			<ul v-if="comments">
				<li
					class="relative flex flex-col group p-2 cursor-pointer ease-in duration-300"
					v-for="comment in comments"
					:key="comment.id"
					:class="{
						'text-right': comment.user.name === store.state.user?.name,
					}"
				>
					<div
						v-if="comment.user.name === store.state.user?.name"
						class="absolute right-0 top-0.5 invisible flex rounded-full bg-red-500 h-4 w-4 cursor-pointer opacity-75 hover:opacity-100 group-hover:visible"
					>
						<span
							class="left-0.5 material-icons cursor-pointer m-auto text-xs text-white"
							@click="removeComment(comment.id)"
						>
							close
						</span>
					</div>
					<span>{{ comment.user.name }}:</span>
					<span
						class="bg-white rounded-lg p-2 dark:bg-gray-700"
						:class="{
							'ml-auto': comment.user.name === store.state.user?.name,
						}"
					>
						{{ comment.message }}
					</span>
				</li>
			</ul>
		</div>
		<div class="w-full mt-2">
			<form class="flex justify-between items-center">
				<TextField class="w-full" placeholder="Text" v-model="message" />
				<button class="p-2" type="button" @click="submitForm">
					<span
						class="material-icons -rotate-45 text-2xl text-gray-500 cursor-pointer hover:text-black dark:text-gray-700 dark:hover:text-white"
					>
						send
					</span>
				</button>
			</form>
		</div>
	</div>
</template>

<script setup lang="ts">
	import TextField from 'src/components/general/TextField.vue';
	import Button from 'src/components/general/Button.vue';
	import { onBeforeMount, onMounted, Ref, ref, watch } from 'vue';
	import { Assignee } from 'src/components/general/AssigneeUsers.vue';
	import store from '../../store';
	import {
		createComment,
		deleteComment,
		getComments,
	} from 'src/actions/tmgr/comments';
	interface Props {
		assignees: Assignee[];
		taskId: number;
		workspaceMembers: Assignee[];
	}
	const props = defineProps<Props>();
	const comments = ref({});
	const message: Ref<string> = ref('');
	const processing = ref(false);

	watch(processing, async (newValue) => {
		if (!newValue) {
			const commentsData = await getComments(props.taskId);
			comments.value = commentsData;
		}
	});

	onBeforeMount(async () => {
		const commentsData = await getComments(props.taskId);
		comments.value = commentsData;
	});

	async function submitForm() {
		processing.value = true;
		const newComment = {
			message: message.value,
		};
		await createComment(props.taskId, newComment);
		message.value = '';
		processing.value = false;
	}
	async function removeComment(id: number) {
		processing.value = true;
		await deleteComment(id);
		processing.value = false;
	}
</script>
