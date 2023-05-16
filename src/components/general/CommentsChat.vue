<template>
	<div class="w-1/2 mt-10 p-4">
		<div class="w-full h-5/6 dark:bg-gray-800 rounded p-4">
			<ul v-if="comments">
				<li class="flex flex-col" v-for="comment in comments" :key="comment.id">
					<span>{{ store.state.user?.name }}</span>
					<span class="bg-white w-max rounded-lg p-2">
						{{ comment.text }}
					</span>
				</li>
			</ul>
		</div>
		<div class="w-full">
			<form class="flex justify-between items-center">
				<TextField class="w-full" placeholder="Text" v-model="commentText" />
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
	import { onMounted, Ref, ref, watch } from 'vue';
	import { Assignee } from 'src/components/general/AssigneeUsers.vue';
	import store from '../../store';
	import { laPlaceOfWorshipSolid } from '@quasar/extras/line-awesome';
	interface Props {
		assignees: Assignee[];
	}
	interface Comment {
		id: string;
		userId: string;
		commentText: string;
	}
	const props = defineProps<Props>();

	const commentText: Ref<string> = ref('');
	const comments = ref(JSON.parse(localStorage.getItem('messages') || '[]'));

	watch(commentText, (newValue) => {
		// console.log(newValue);
		// console.log(props.assignees);
	});
	onMounted(() => {
		// const messages = comments.value.map(el=>el)
		// console.log(messages);
	});

	function submitForm() {
		const id = Date.now();
		const userId = store.state.user.id;
		if (comments.value.length) {
			comments.value.push({
				id,
				userId,
				text: commentText.value,
			});
			localStorage.setItem('messages', JSON.stringify(comments.value));
		} else {
			comments.value.push({
				id,
				userId,
				text: commentText.value,
			});
			localStorage.setItem('messages', JSON.stringify(comments.value));
		}
		// if (localStorage.getItem('messages')) {
		// 	console.log('YES');
		// 	const oldComments = JSON.parse(localStorage.getItem('messages')!);
		// 	console.log(oldComments);

		// }

		// localStorage.setItem(
		// 	'messages',
		// 	JSON.stringify([{ id, userId, text: commentText.value }]),
		// );
		// console.log(localStorage.getItem('messages'));

		// console.log(user);

		// Clear the commentText input
		commentText.value = '';
	}
</script>
