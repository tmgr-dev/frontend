<template>
	<details
		class="border-t border-line pt-2 text-sm text-ink"
		@toggle="loadViews"
	>
		<summary class="cursor-pointer font-medium">
			{{ $t('general.filtersBoard.savedViews') }}
		</summary>
		<div class="mt-2 flex flex-col gap-2">
			<div v-for="view in views" :key="view.id" class="flex items-center gap-2">
				<button
					type="button"
					class="min-w-0 flex-1 truncate text-left text-brand"
					:title="view.name"
					@click="$emit('apply', view.query)"
				>
					{{ view.name }}
				</button>
				<button
					type="button"
					:disabled="busy"
					@click="
						editing = view.id;
						name = view.name;
					"
				>
					{{ $t('general.filtersBoard.rename') }}
				</button>
				<button type="button" :disabled="busy" @click="persist(view.id, null)">
					{{ $t('common.actions.delete') }}
				</button>
			</div>
			<form class="flex items-center gap-2" @submit.prevent="save">
				<input
					v-model="name"
					class="min-w-0 flex-1 rounded-md border border-line bg-surface px-2 py-1 text-ink"
					:aria-label="$t('general.filtersBoard.viewName')"
					:placeholder="$t('general.filtersBoard.viewName')"
					required
				/>
				<button
					type="submit"
					:disabled="busy || !name.trim()"
					class="text-brand disabled:opacity-50"
				>
					{{
						editing
							? $t('common.actions.save')
							: $t('general.filtersBoard.saveView')
					}}
				</button>
				<button v-if="editing" type="button" :disabled="busy" @click="reset">
					{{ $t('common.actions.cancel') }}
				</button>
			</form>
			<p v-if="error" role="alert">{{ $t('common.errors.generic') }}</p>
		</div>
	</details>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import { getUserSettings, patchUserSettings } from '@/actions/tmgr/user';
	import { isLocalWorkspaceId } from '@/local/classify';
	import { boardViewQuery, type BoardViewQuery } from '@/utils/boardViewQuery';

	type View = { name: string; query: BoardViewQuery };
	type Entry = View & { id: string };

	export default defineComponent({
		props: {
			workspaceId: { type: Number, required: true },
			userId: { type: Number, required: true },
		},
		emits: ['apply'],
		data() {
			return {
				localViews: {} as Record<string, View>,
				editing: '',
				name: '',
				busy: false,
				error: false,
			};
		},
		computed: {
			prefix(): string {
				return 'boardView.' + this.workspaceId + '.' + this.userId + '.';
			},
			localKey(): string {
				return 'tmgr.boardViews.' + this.userId + '.' + this.workspaceId;
			},
			views(): Entry[] {
				const settings = isLocalWorkspaceId(this.workspaceId)
					? this.localViews
					: this.$store.state.userSettings;
				return Object.entries(settings ?? {})
					.flatMap(([id, value]) => {
						const view = value as Partial<View> | null;
						return id.startsWith(this.prefix) &&
							typeof view?.name === 'string' &&
							view.query &&
							typeof view.query === 'object'
							? [{ id, name: view.name, query: view.query }]
							: [];
					})
					.sort((a, b) => a.name.localeCompare(b.name));
			},
		},
		methods: {
			reset() {
				this.editing = '';
				this.name = '';
			},
			async loadViews(event: Event) {
				if (!(event.currentTarget as HTMLDetailsElement).open || this.busy)
					return;
				this.busy = true;
				this.error = false;
				try {
					if (isLocalWorkspaceId(this.workspaceId)) {
						this.localViews =
							JSON.parse(localStorage.getItem(this.localKey) || '{}') ?? {};
					} else {
						await getUserSettings();
					}
				} catch {
					this.error = true;
				} finally {
					this.busy = false;
				}
			},
			async save() {
				const name = this.name.trim();
				if (!name || this.busy) return;
				const previous = this.views.find((v) => v.id === this.editing);
				if (this.editing && !previous) {
					this.reset();
					return;
				}
				await this.persist(
					previous?.id ??
						this.prefix + crypto.getRandomValues(new Uint32Array(4)).join('-'),
					{
						name,
						query: previous?.query ?? boardViewQuery(this.$store.state.filter),
					},
				);
			},
			async persist(id: string, value: View | null) {
				if (this.busy) return;
				this.busy = true;
				this.error = false;
				const generation = this.$store.state.sessionGeneration;
				try {
					if (isLocalWorkspaceId(this.workspaceId)) {
						const views =
							JSON.parse(localStorage.getItem(this.localKey) || '{}') ?? {};
						if (value) views[id] = value;
						else delete views[id];
						localStorage.setItem(this.localKey, JSON.stringify(views));
						this.localViews = views;
					} else {
						await patchUserSettings({ [id]: value });
						if (
							this.$store.state.sessionGeneration !== generation ||
							this.$store.state.user?.id !== this.userId
						)
							return;
						this.$store.commit('setUserSettings', {
							...this.$store.state.userSettings,
							[id]: value,
						});
					}
					if (value || this.editing === id) this.reset();
				} catch {
					this.error = true;
				} finally {
					this.busy = false;
				}
			},
		},
	});
</script>
