<template>
	<div class="mt-10 px-4">
		<div
			class="h-72 max-h-80 w-full overflow-y-scroll rounded p-4 dark:bg-gray-800"
		>
			<ul v-if="comments">
				<li class="p-2" v-for="comment in comments" :key="comment.id">
					<div
						class="group relative flex w-fit flex-col"
						:class="{
							'ml-auto': comment.user.name === store.state.user?.name,
						}"
					>
						<span
							class="'dark:bg-inherit p-0 text-sm text-gray-400"
							v-if="
								comment.message.toLowerCase().includes('timer') ||
								comment.message.toLowerCase().includes('task')
							"
						>
							{{ comment.message }}
							{{ moment(comment.updated_at).toNow() }} ago.
						</span>

						<div v-else class="flex flex-col">
							<div>
								<div
									v-if="comment.user.name === store.state.user?.name"
									class="invisible absolute -right-4 -top-0 flex h-4 w-4 cursor-pointer rounded-full bg-red-500 opacity-75 duration-300 ease-in hover:opacity-100 group-hover:visible"
								>
									<span
										class="material-icons left-0.5 m-auto cursor-pointer text-xs text-white"
										@click="removeComment(comment.id!)"
									>
										close
									</span>
								</div>

								<div
									v-if="comment.user.name === store.state.user?.name"
									class="invisible absolute -right-4 top-6 flex h-4 w-4 cursor-pointer rounded-full bg-green-500 opacity-75 duration-300 ease-in hover:opacity-100 group-hover:visible"
								>
									<span
										class="material-icons left-0.5 m-auto cursor-pointer text-xs text-white"
										@click="onEditClick(comment.id)"
									>
										edit
									</span>
								</div>
								<div
									class="invisible absolute -right-6 -bottom-4 flex cursor-pointer opacity-75 duration-300 ease-in hover:opacity-100 group-hover:visible"
								>
									<span
										class="material-icons text-md left-0.5 z-50 m-auto h-6 w-6 cursor-pointer text-white"
										@click="onReplyClick(comment.id)"
									>
										reply
									</span>
								</div>
							</div>

							<div class="flex flex-col">
								<span>{{ comment.user.name }}:</span>
								<span
									class="w-fit cursor-pointer rounded-lg p-2 dark:bg-gray-700"
								>
									{{ comment.message }}
								</span>
							</div>
						</div>
					</div>
					<div
						class="z-10 flex w-fit rounded px-2 text-xs opacity-70 dark:bg-gray-500"
						:class="{
							'ml-auto': comment.user.name === store.state.user?.name,
						}"
						v-if="comment.parent_comment"
					>
						<span>
							{{ comment.parent_comment.message }}
						</span>
					</div>
				</li>
			</ul>

			<div ref="$chatAnchor" />
		</div>
		<div class="mt-2 w-full">
			<Transition
				enter-active-class="transition-opacity duration-300 ease-out"
				enter-from-class="opacity-0"
				leave-active-class="transition-opacity duration-300 ease-out"
				leave-to-class="opacity-0"
			>
				<div
					class="relative m-2 w-fit rounded-lg p-2 dark:bg-gray-700"
					v-if="isReplying"
				>
					<span class="w-fit cursor-pointer">
						{{ replyingMessage.message }}
					</span>
					<span
						class="left-18 material-icons text-md absolute m-auto h-6 w-6 cursor-pointer text-white"
					>
						reply
					</span>
				</div>
			</Transition>
			<form class="flex items-center justify-between">
				<TextField
					class="w-full"
					placeholder="Text"
					v-model="message"
					@keydown="submitCommentByKeys"
				/>
				<button class="p-2" type="button" @click="submitForm">
					<span
						class="material-icons -rotate-45 cursor-pointer text-2xl text-gray-500 hover:text-black dark:text-gray-700 dark:hover:text-white"
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
	const message: Ref = ref('');
	const processing = ref(false);
	const isReplying = ref(false);
	const replyingMessage = ref<Comment>();
	const isEditing = ref(false);
	const editingMessage = ref<Comment>();
	const $chatAnchor = ref<HTMLElement>();

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
		comments.value = (await getComments(props.taskId)) as Comment[];
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
			newComment.comment_id = replyingMessage.value?.id;
			await createComment(props.taskId, newComment);
			isReplying.value = false;
		} else {
			await createComment(props.taskId, newComment);
		}
		// it won't work properly cuz we need to add the comment before sending the request
		$chatAnchor.value?.scrollIntoView(false);
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
	const submitCommentByKeys = (e: KeyboardEvent) => {
		if (e.key === 'Enter') {
			e.preventDefault();

			if (message.value) {
				submitForm();
			}
		}
	};

	defineExpose({
		addingComment,
		deletingComment,
		editingComment,
	});
</script>
