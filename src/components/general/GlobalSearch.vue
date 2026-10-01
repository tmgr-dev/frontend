<template>
	<div>
		<button
			type="button"
			class="-ml-1 flex h-7 w-7 items-center justify-center rounded-md text-ink-subtle hover:bg-accent hover:text-accent-foreground"
			aria-label="Поиск"
			title="Поиск"
			@click="open = true"
		>
			<Search class="h-4 w-4" />
		</button>
		<Dialog v-model:open="open">
			<DialogContent class="max-w-xl gap-3 p-4">
				<DialogHeader>
					<DialogTitle>Поиск</DialogTitle>
				</DialogHeader>
				<div class="flex gap-1" role="tablist">
					<button
						v-for="item in tabs"
						:key="item.key"
						type="button"
						role="tab"
						:aria-selected="activeTab === item.key"
						class="rounded-md px-3 py-1 text-sm"
						:class="
							activeTab === item.key
								? 'bg-accent font-medium text-accent-foreground'
								: 'text-muted-foreground hover:bg-accent/60'
						"
						@click="tab = item.key"
					>
						{{ item.label }}
					</button>
				</div>
				<input
					v-model="query"
					data-selectable
					autofocus
					class="h-9 rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring dark:border-input dark:bg-background"
					:placeholder="
						activeTab === 'pages' ? 'Название или текст страницы' : 'Название задачи'
					"
				/>
				<div class="max-h-80 overflow-y-auto">
					<p v-if="!canRun" class="px-1 py-2 text-sm text-muted-foreground">
						Введите минимум {{ minLength }} символа.
					</p>
					<p v-else-if="loading" class="px-1 py-2 text-sm text-muted-foreground">
						Ищем…
					</p>
					<p v-else-if="error" class="px-1 py-2 text-sm text-destructive">
						Не удалось выполнить поиск
					</p>
					<p
						v-else-if="!taskHits.length && !pageHits.length"
						class="px-1 py-2 text-sm text-muted-foreground"
					>
						Ничего не найдено
					</p>
					<ul v-else-if="activeTab === 'pages'" class="divide-y divide-border">
						<li v-for="hit in pageHits" :key="hit.id">
							<router-link
								:to="pageHitUrl(workspaceCode, hit)"
								class="block rounded-md px-2 py-2 hover:bg-accent"
								@click="open = false"
							>
								<div class="flex items-center gap-2">
									<span class="min-w-0 flex-1 truncate text-sm font-medium">{{
										hit.title
									}}</span>
									<span
										class="shrink-0 rounded bg-muted px-1.5 py-0.5 text-2xs text-muted-foreground"
										>{{ pageTypeLabel(hit.type) }}</span
									>
								</div>
								<p
									v-if="hit.snippet"
									class="mt-0.5 line-clamp-2 text-xs text-muted-foreground"
								>
									{{ hit.snippet }}
								</p>
								<p class="mt-0.5 text-2xs text-muted-foreground">
									{{ formatDate(hit.updated_at) }}
								</p>
							</router-link>
						</li>
					</ul>
					<ul v-else class="divide-y divide-border">
						<li v-for="task in taskHits" :key="task.id">
							<router-link
								:to="taskUrl(task)"
								class="block rounded-md px-2 py-2 text-sm hover:bg-accent"
								@click="open = false"
							>
								{{ task.title }}
							</router-link>
						</li>
					</ul>
				</div>
			</DialogContent>
		</Dialog>
	</div>
</template>

<script lang="ts">
	import { searchPages, type PageSearchHit } from '@/actions/tmgr/pages';
	import { getTasks, type Task } from '@/actions/tmgr/tasks';
	import {
		Dialog,
		DialogContent,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import {
		availableSearchTabs,
		canSearch,
		MIN_SEARCH_LENGTH,
		normalizeQuery,
		pageHitUrl,
		pageTypeLabel,
		resolveSearchTab,
		SEARCH_DEBOUNCE_MS,
		type SearchTab,
	} from '@/utils/pagesSearch';
	import { generateTaskUrl } from '@/utils/url';
	import { Search } from 'lucide-vue-next';
	import { computed, defineComponent, onBeforeUnmount, ref, watch } from 'vue';

	export default defineComponent({
		name: 'GlobalSearch',
		components: { Dialog, DialogContent, DialogHeader, DialogTitle, Search },
		props: {
			workspaceCode: { type: String, required: true },
			pagesEnabled: { type: Boolean, default: false },
		},
		setup(props) {
			const open = ref(false);
			const tab = ref<SearchTab>('tasks');
			const query = ref('');
			const loading = ref(false);
			const error = ref(false);
			const taskHits = ref<Task[]>([]);
			const pageHits = ref<PageSearchHit[]>([]);
			let timer: ReturnType<typeof setTimeout> | null = null;
			let request = 0;

			const tabs = computed(() => availableSearchTabs(props.pagesEnabled));
			const activeTab = computed(() =>
				resolveSearchTab(tab.value, props.pagesEnabled),
			);
			const canRun = computed(() => canSearch(query.value));

			const run = async () => {
				const current = ++request;
				const q = normalizeQuery(query.value);
				const target = activeTab.value;
				loading.value = true;
				error.value = false;
				try {
					if (target === 'pages') {
						const hits = await searchPages(q, { limit: 20 });
						if (current === request) pageHits.value = hits;
					} else {
						const response = await getTasks({
							page: 1,
							per_page: 20,
							params: { search: q },
						});
						if (current === request) taskHits.value = response.data;
					}
				} catch {
					if (current === request) error.value = true;
				} finally {
					if (current === request) loading.value = false;
				}
			};

			const schedule = () => {
				if (timer) clearTimeout(timer);
				++request;
				taskHits.value = [];
				pageHits.value = [];
				if (!canSearch(query.value)) {
					loading.value = false;
					return;
				}
				loading.value = true;
				timer = setTimeout(run, SEARCH_DEBOUNCE_MS);
			};

			watch([query, activeTab], schedule);
			watch(open, (isOpen) => {
				if (!isOpen) query.value = '';
			});
			onBeforeUnmount(() => {
				if (timer) clearTimeout(timer);
			});

			const taskUrl = (task: Task) =>
				generateTaskUrl(
					task.id as number,
					{ code: props.workspaceCode },
					typeof task.category === 'object' ? task.category : null,
				);

			const formatDate = (value: string) =>
				value ? new Date(value).toLocaleDateString() : '';

			return {
				open,
				tab,
				tabs,
				activeTab,
				query,
				loading,
				error,
				canRun,
				taskHits,
				pageHits,
				minLength: MIN_SEARCH_LENGTH,
				pageHitUrl,
				pageTypeLabel,
				taskUrl,
				formatDate,
			};
		},
	});
</script>
