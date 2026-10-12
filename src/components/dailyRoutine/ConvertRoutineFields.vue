<template>
	<div class="flex flex-col gap-2">
		<select
			v-model.number="workspaceId"
			aria-label="Workspace"
			class="w-full rounded-md border border-line bg-surface-sunken px-3 py-2 text-sm text-ink focus:outline-none"
			@change="loadCategories"
		>
			<option v-for="w in workspaceOptions" :key="w.id" :value="w.id">
				{{ w.name }}
			</option>
		</select>
		<select
			v-model="categoryId"
			aria-label="Category"
			class="w-full rounded-md border border-line bg-surface-sunken px-3 py-2 text-sm text-ink focus:outline-none"
		>
			<option :value="null">No category</option>
			<option v-for="c in categories" :key="c.id" :value="c.id">
				{{ c.title }}
			</option>
		</select>
		<button
			type="button"
			class="rounded-md bg-brand px-3 py-2 text-sm font-semibold text-white transition-colors hover:bg-brand-hover disabled:cursor-not-allowed disabled:bg-surface-sunken disabled:text-ink-subtle"
			:disabled="!workspaceId || busy"
			@click="onConvert"
		>
			Convert
		</button>
	</div>
</template>

<script lang="ts">
	import {
		getWorkspaceCategories,
		type Category,
	} from '@/actions/tmgr/categories';
	import { getWorkspaces } from '@/actions/tmgr/workspaces';
	import { defineComponent, onMounted, ref } from 'vue';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'ConvertRoutineFields',
		props: {
			busy: { type: Boolean, default: false },
		},
		emits: ['convert'],
		setup(_props, { emit }) {
			const store = useStore();
			const workspaceId = ref<number | null>(
				store.getters.currentWorkspaceId ??
					store.state.workspaces?.[0]?.id ??
					null,
			);
			const categoryId = ref<number | null>(null);
			const categories = ref<Category[]>([]);
			const workspaceOptions = ref<{ id: number; name: string }[]>(
				store.state.workspaces ?? [],
			);

			async function loadCategories() {
				categoryId.value = null;
				categories.value = [];
				const requested = workspaceId.value;
				if (!requested) return;
				try {
					const list = await getWorkspaceCategories(requested);
					// A slower response for a workspace the user already left.
					if (requested === workspaceId.value) categories.value = list;
				} catch {
					// category list is optional — conversion works without one
				}
			}

			function onConvert() {
				if (!workspaceId.value) return;
				emit('convert', {
					workspaceId: workspaceId.value,
					projectCategoryId: categoryId.value,
				});
			}

			onMounted(() => {
				// The local API rejects any target other than the active local workspace with 409.
				getWorkspaces()
					.then((workspaces) => {
						const current = workspaces.find(
							(w) => Number(w.id) === Number(workspaceId.value),
						);
						workspaceOptions.value = current?.is_local ? [current] : workspaces;
					})
					.catch(() => {});
				loadCategories();
			});

			return {
				workspaceId,
				categoryId,
				categories,
				workspaceOptions,
				loadCategories,
				onConvert,
			};
		},
	});
</script>
