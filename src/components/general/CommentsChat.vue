<template>
	<div class="mt-10 px-4">
		<div
			class="w-full h-64 max-h-80 overflow-y-scroll dark:bg-gray-800 rounded p-4"
		>
			<ul v-if="comments">
				<li class="relative p-2" v-for="comment in comments" :key="comment.id">
					<div
						class="w-fit flex flex-col group"
						:class="{
							'ml-auto': comment.user.name === store.state.user?.name,
						}"
					>
						<span
							class="'dark:bg-inherit text-sm p-0 text-gray-400"
							v-if="
								comment.message.toLowerCase().includes('timer') ||
								comment.message.toLowerCase().includes('task')
							"
						>
							{{ comment.message }}
							{{ moment(comment.updated_at).toNow() }} ago.
						</span>

						<div v-else class="flex flex-col">
							<div
								v-if="comment.user.name === store.state.user?.name"
								class="absolute ease-in duration-300 invisible right-1 top-0.5 flex rounded-full bg-red-500 h-4 w-4 cursor-pointer opacity-75 hover:opacity-100 group-hover:visible"
							>
								<span
									class="left-0.5 material-icons cursor-pointer m-auto text-xs text-white"
									@click="removeComment(comment.id)"
								>
									close
								</span>
							</div>
							<span> {{ comment.user.name }}: </span>
							<span
								class="w-fit rounded-lg p-2 dark:bg-gray-700 cursor-pointer"
							>
								{{ comment.message }}
							</span>
						</div>
					</div>
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
	import moment from 'moment';
	import {
		createComment,
		deleteComment,
		getComments,
	} from 'src/actions/tmgr/comments';
	interface Props {
		assignees: Assignee[];
		taskId: number;
		workspaceMembers: Assignee[];
		startTime: number;
		isDataEdited: boolean;
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
	watch(
		() => props.isDataEdited,
		async (newValue) => {
			if (newValue) {
				processing.value = true;
				const newComment = {
					message: `Task changed `,
				};
				await createComment(props.taskId, newComment);
				processing.value = false;
			}
		},
	);
	watch(
		() => props.startTime,
		async (newValue, oldValue) => {
			if (newValue !== 0) {
				processing.value = true;
				const newComment = {
					message: `Timer starts`,
				};
				await createComment(props.taskId, newComment);
				processing.value = false;
			}
		},
	);

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
