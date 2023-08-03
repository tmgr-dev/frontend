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
					class="relative m-3 w-fit rounded-lg p-2 dark:bg-gray-700"

					v-if="isReplying"
				>
					<span class="w-fit cursor-pointer">
						{{ replyingMessage.message }}
					</span>
					<span

						class="material-icons text-md left-18 absolute m-auto h-6 w-6 cursor-pointer text-white"

					>
						reply
					</span>
					<div
						class="absolute -left-2 -top-1.5 flex h-4 w-4 cursor-pointer rounded-full bg-red-500 opacity-75 duration-300 ease-in hover:opacity-100"
					>
						<span
							class="material-icons left-0.5 m-auto cursor-pointer text-xs text-white"
							@click="closeReply"
						>
							close
						</span>
					</div>
				</div>
			</Transition>
			<form class="flex items-center justify-between">
				<TextField
					class="w-full"
					:placeholder="isHelpMode ? 'Type question to AI Assistant' : 'Type comment here...'"
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
				<button class="py-2" type="button">
					<svg
						@click="toggleHelpMode"
						xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"
						class="w-6 h-6"
						:class="{
							'opacity-20': !isHelpMode,
							'hover:opacity-50': !isHelpMode
						}"
					>
						<path fill-rule="evenodd" d="M9 4.5a.75.75 0 01.721.544l.813 2.846a3.75 3.75 0 002.576 2.576l2.846.813a.75.75 0 010 1.442l-2.846.813a3.75 3.75 0 00-2.576 2.576l-.813 2.846a.75.75 0 01-1.442 0l-.813-2.846a3.75 3.75 0 00-2.576-2.576l-2.846-.813a.75.75 0 010-1.442l2.846-.813A3.75 3.75 0 007.466 7.89l.813-2.846A.75.75 0 019 4.5zM18 1.5a.75.75 0 01.728.568l.258 1.036c.236.94.97 1.674 1.91 1.91l1.036.258a.75.75 0 010 1.456l-1.036.258c-.94.236-1.674.97-1.91 1.91l-.258 1.036a.75.75 0 01-1.456 0l-.258-1.036a2.625 2.625 0 00-1.91-1.91l-1.036-.258a.75.75 0 010-1.456l1.036-.258a2.625 2.625 0 001.91-1.91l.258-1.036A.75.75 0 0118 1.5zM16.5 15a.75.75 0 01.712.513l.394 1.183c.15.447.5.799.948.948l1.183.395a.75.75 0 010 1.422l-1.183.395c-.447.15-.799.5-.948.948l-.395 1.183a.75.75 0 01-1.422 0l-.395-1.183a1.5 1.5 0 00-.948-.948l-1.183-.395a.75.75 0 010-1.422l1.183-.395c.447-.15.799-.5.948-.948l.395-1.183A.75.75 0 0116.5 15z" clip-rule="evenodd" />
					</svg>
				</button>
			</form>
		</div>
	</div>
</template>

<script setup lang="ts">
	import TextField from 'src/components/general/TextField.vue';
	import Button from 'src/components/general/Button.vue';
	import { onBeforeMount, onMounted, Ref, ref, UnwrapRef, watch } from 'vue';
	import store from '../../store';
	import moment from 'moment';
	import {
		createComment,
		deleteComment,
		getComments,
		updateComment,
		createAskingHelpComment
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
	const isHelpMode = ref(false);
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
			if (newValue && newValue !== 0) {
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
  
	const closeReply = () => {
		isReplying.value = !isReplying.value;
	};

	const toggleHelpMode = () => {
		isHelpMode.value = !isHelpMode.value;

	};
	const onEditClick = (id: number) => {
		isEditing.value = !isEditing.value;
		editingMessage.value = comments.value.find((comment) => comment.id === id);
	};

	async function submitForm() {
		processing.value = true;

		if (isHelpMode.value) {
			await createAskingHelpComment(props.taskId, message.value);
			processing.value = false;
			return;
		}

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
