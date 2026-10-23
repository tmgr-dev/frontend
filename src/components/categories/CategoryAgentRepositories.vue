<template>
	<div
		v-if="enabled"
		class="mt-6 border-t border-line pt-6"
		data-testid="category-agent-repositories"
	>
		<h3 class="mb-1 text-base font-semibold text-ink">
			Agent run repositories
		</h3>
		<p class="mb-4 text-sm text-ink-muted">
			Repositories agents may work on for tasks of this category. Leave empty to
			use the linked GitHub repository.
		</p>
		<textarea
			v-model="text"
			rows="4"
			spellcheck="false"
			placeholder="owner/name"
			:disabled="loading || saving"
			class="w-full rounded-card border border-line bg-surface px-3 py-2 font-mono text-sm text-ink outline-none"
			data-testid="agent-repositories-input"
		></textarea>
		<button
			type="button"
			:disabled="loading || saving || !dirty"
			class="mt-3 inline-flex h-9 items-center justify-center rounded-pill bg-brand px-4 text-sm font-semibold text-on-accent shadow-tmgr-xs transition-colors hover:bg-brand-hover disabled:cursor-not-allowed disabled:opacity-50"
			data-testid="agent-repositories-save"
			@click="save"
		>
			<span v-if="saving">Saving…</span>
			<span v-else>Save</span>
		</button>
		<div
			v-if="error"
			class="mt-4 rounded-card border border-status-fix/40 bg-status-fix-bg p-4"
		>
			<p
				class="text-sm text-status-fix-fg"
				data-testid="agent-repositories-error"
			>
				{{ error }}
			</p>
		</div>
	</div>
</template>

<script lang="ts">
	import {
		getAgentRunsSetting,
		getCategoryAgentRepositories,
		setCategoryAgentRepositories,
	} from '@/actions/tmgr/agentRuns';
	import store from '@/store';
	import { apiErrorText } from '@/utils/apiErrorText';
	import { computed, defineComponent, onMounted, ref } from 'vue';

	const parse = (value: string): string[] =>
		value
			.split('\n')
			.map((line) => line.trim())
			.filter(Boolean);

	export default defineComponent({
		name: 'CategoryAgentRepositories',
		props: { categoryId: { type: [Number, String], required: true } },
		setup(props) {
			const enabled = ref(false);
			const loading = ref(true);
			const saving = ref(false);
			const error = ref('');
			const text = ref('');
			const saved = ref<string[]>([]);

			const dirty = computed(
				() => parse(text.value).join('\n') !== saved.value.join('\n'),
			);

			const apply = (list: string[]) => {
				saved.value = list;
				text.value = list.join('\n');
			};

			onMounted(async () => {
				try {
					enabled.value = await getAgentRunsSetting(
						store.getters.currentWorkspaceId,
					);
					if (enabled.value)
						apply(await getCategoryAgentRepositories(props.categoryId));
				} catch {
					enabled.value = false;
				} finally {
					loading.value = false;
				}
			});

			const save = async () => {
				saving.value = true;
				error.value = '';
				try {
					apply(
						await setCategoryAgentRepositories(
							props.categoryId,
							parse(text.value),
						),
					);
				} catch (failure) {
					error.value = apiErrorText(
						failure,
						'Could not save the repositories.',
					);
				} finally {
					saving.value = false;
				}
			};

			return { enabled, loading, saving, error, text, dirty, save };
		},
	});
</script>
