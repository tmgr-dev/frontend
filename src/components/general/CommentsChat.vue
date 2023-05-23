<template>
	<div class="mt-10 px-4">
		<div
			class="w-full h-72 max-h-80 overflow-y-scroll dark:bg-gray-800 rounded p-4">
			<ul v-if="comments">
				<li class="p-2" v-for="comment in comments" :key="comment.id">
					<div
						class="w-fit relative flex flex-col group"
						:class="{
							'ml-auto': comment.user.name === store.state.user?.name,
						}">
						<span
							class="'dark:bg-inherit text-sm p-0 text-gray-400"
							v-if="
								comment.message.toLowerCase().includes('timer') ||
								comment.message.toLowerCase().includes('task')
							">
							{{ comment.message }}
							{{ moment(comment.updated_at).toNow() }} ago.
						</span>

						<div v-else class="flex flex-col">
							<div>
								<div
									v-if="comment.user.name === store.state.user?.name"
									class="absolute ease-in duration-300 invisible -right-4 -top-0 flex rounded-full bg-red-500 h-4 w-4 cursor-pointer opacity-75 hover:opacity-100 group-hover:visible">
									<span
										class="left-0.5 material-icons cursor-pointer m-auto text-xs text-white"
										@click="removeComment(comment.id!)">
										close
									</span>
								</div>

								<div
									v-if="comment.user.name === store.state.user?.name"
									class="absolute ease-in duration-300 invisible -right-4 top-6 flex rounded-full bg-green-500 h-4 w-4 cursor-pointer opacity-75 hover:opacity-100 group-hover:visible">
									<span
										class="left-0.5 material-icons cursor-pointer m-auto text-xs text-white"
										@click="onEditClick(comment.id)">
										edit
									</span>
								</div>
								<div
									class="absolute ease-in duration-300 invisible -right-6 -bottom-4 flex cursor-pointer opacity-75 hover:opacity-100 group-hover:visible">
									<span
										class="left-0.5 h-6 w-6 material-icons cursor-pointer m-auto text-md text-white z-50"
										@click="onReplyClick(comment.id)">
										reply
									</span>
								</div>
							</div>

							<div class="flex flex-col">
								<span>{{ comment.user.name }}:</span>
								<span
									class="w-fit rounded-lg p-2 dark:bg-gray-700 cursor-pointer">
									{{ comment.message }}
								</span>
							</div>
						</div>
					</div>
					<div
						class="flex w-fit px-2 rounded dark:bg-gray-500 text-xs opacity-70 z-10"
						:class="{
							'ml-auto': comment.user.name === store.state.user?.name,
						}"
						v-if="comment.parent_comment">
						<span>
							{{ comment.parent_comment.message }}
						</span>
					</div>
				</li>
			</ul>
		</div>
		<div class="w-full mt-2">
			<Transition
				enter-active-class="transition-opacity duration-300 ease-out"
				enter-from-class="opacity-0"
				leave-active-class="transition-opacity duration-300 ease-out"
				leave-to-class="opacity-0">
				<div
					class="m-2 w-fit relative rounded-lg p-2 dark:bg-gray-700"
					v-if="isReplying">
					<span class="w-fit cursor-pointer">
						{{ replyingMessage.message }}
					</span>
					<span
						class="absolute left-18 h-6 w-6 material-icons cursor-pointer m-auto text-md text-white">
						reply
					</span>
				</div>
			</Transition>
			<form class="flex justify-between items-center">
				<TextField class="w-full" placeholder="Text" v-model="message" />
				<button class="p-2" type="button" @click="submitForm">
					<span
						class="material-icons -rotate-45 text-2xl text-gray-500 cursor-pointer hover:text-black dark:text-gray-700 dark:hover:text-white">
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
	import { onBeforeMount, onMounted, Ref, ref, UnwrapRef, watch } from 'vue';
	// import { Assignee } from 'src/components/general/AssigneeUsers.vue';
	import store from '../../store';
	import moment from 'moment';
	import {
		createComment,
		deleteComment,
		getComments,
		updateComment,
	} from 'src/actions/tmgr/comments';
	import { defineExpose } from 'vue';
	export interface Assignee {
		id: number;
		name: string;
	}
	interface Message {
		message: string;
	}
	interface Comment {
		id: number;
		message: string;
		task_id: number;
		user: Assignee;
		comment_id?: number;
	}

	interface Props {
		assignees: Assignee[];
		taskId: number;
		workspaceMembers: Assignee[];
		startTime: number;
		isDataEdited: boolean;
		isProcessing: boolean;
		comment: Comment;
	}
	const props = defineProps<Props>();
	const comments = ref<Comment[]>([]);
	const message: Ref<string> = ref('');
	const processing = ref<boolean>(false);
	const isReplying = ref<boolean>(false);
	const replyingMessage = ref<Comment>();
	const isEditing = ref<boolean>(false);
	const editingMessage = ref<Comment>();

	watch(processing, async (newValue) => {
		if (!newValue) {
			message.value = '';
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
	watch(isEditing, (nw) => {
		message.value = editingMessage.value?.message!;
	});

	onBeforeMount(async () => {
		const commentsData = (await getComments(props.taskId)) as Comment[];
		comments.value = commentsData;
	});
	const onReplyClick = (id: number) => {
		isReplying.value = !isReplying.value;
		replyingMessage.value = comments.value.find((comment) => comment.id === id);
	};
	const onEditClick = (id: number) => {
		isEditing.value = !isEditing.value;
		editingMessage.value = comments.value.find((comment) => comment.id === id);
	};

	async function submitForm() {
		processing.value = true;
		const newComment: any = {
			message: message.value,
		};
		if (isEditing.value) {
			await updateComment(editingMessage.value?.id!, newComment);
			isEditing.value = false;
		} else if (isReplying.value) {
			(newComment.comment_id = replyingMessage.value?.id),
				await createComment(props.taskId, newComment);
			isReplying.value = false;
		} else {
			await createComment(props.taskId, newComment);
		}
		processing.value = false;
	}

	async function removeComment(id: number) {
		processing.value = true;
		await deleteComment(id);
		processing.value = false;
	}
	const addingComment = (c: Comment) => {
		comments.value.push(c);
	};
	const deletingComment = (c: Comment) => {
		comments.value = comments.value.filter((com) => {
			return com.id !== c.id;
		});
	};
	const editingComment = (c: Comment) => {
		comments.value = comments.value.map((comment) =>
			comment.id === c.id ? { ...comment, message: c.message } : comment,
		);
	};

	defineExpose({
		addingComment,
		deletingComment,
		editingComment,
	});
</script>
