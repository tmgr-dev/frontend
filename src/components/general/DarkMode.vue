<script setup lang="ts">
	import { Moon, Sun } from 'lucide-vue-next';
	import store from '@/store';
	import { computed } from 'vue';

	const isDefaultTheme = computed(
		() => (store.state.theme || 'default') === 'default',
	);
	const switchOn = computed({
		get() {
			return store.state.colorScheme === 'dark';
		},
		set(value) {
			if (!isDefaultTheme.value) return;
			store.commit('setColorScheme', value ? 'dark' : 'default');
		},
	});
</script>

<template>
	<a v-if="isDefaultTheme" href="#" @click="switchOn = !switchOn">
		<div v-if="switchOn" class="flex">
			<Sun class="size-4" />
			<span class="ml-2">Light mode</span>
		</div>
		<div v-else class="flex">
			<Moon class="size-4" />
			<span class="ml-2">Dark mode</span>
		</div>
	</a>
</template>
