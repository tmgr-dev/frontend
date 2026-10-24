<template>
	<div>
		<div v-if="loading" class="flex items-center justify-center py-8">
			<div
				class="h-8 w-8 animate-spin rounded-full border-4 border-brand border-t-transparent"
			></div>
		</div>

		<div
			v-else-if="!hasActivity"
			class="rounded-lg border border-line-strong bg-surface-sunken p-8 text-center"
		>
			<svg
				class="mx-auto mb-3 h-12 w-12 text-ink-subtle"
				fill="none"
				stroke="currentColor"
				viewBox="0 0 24 24"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					stroke-width="2"
					d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"
				></path>
			</svg>
			<p class="text-ink-muted">No git activity linked to this task yet</p>
			<p class="mt-2 text-sm text-ink-subtle">
				Use {{ categoryCode }}-{{ taskId }} in commits, branches, or PR titles
				to link them
			</p>
		</div>

		<div v-else class="space-y-6">
			<div v-if="activity.branches && activity.branches.length > 0">
				<h4 class="mb-3 flex items-center text-sm font-semibold text-ink-muted">
					<svg
						class="mr-2 h-5 w-5"
						fill="none"
						stroke="currentColor"
						viewBox="0 0 24 24"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							stroke-width="2"
							d="M8 7h12m0 0l-4-4m4 4l-4 4m0 6H4m0 0l4 4m-4-4l4-4"
						></path>
					</svg>
					Branches ({{ activity.branches.length }})
				</h4>
				<div class="space-y-2">
					<a
						v-for="branch in activity.branches"
						:key="branch.id"
						:href="branch.repository_url"
						target="_blank"
						class="flex items-center justify-between rounded-lg border border-line bg-surface p-3 transition hover:border-line-strong hover:bg-surface-hover"
					>
						<div class="flex items-center">
							<span
								:class="[
									'mr-3 rounded px-2 py-1 font-mono text-xs',
									branch.is_default
										? 'bg-status-done-bg text-status-done-fg'
										: 'bg-surface-sunken text-ink-muted',
								]"
							>
								{{ branch.name }}
							</span>
							<span v-if="branch.repository" class="text-xs text-ink-subtle">
								{{ branch.repository }}
							</span>
						</div>
						<svg
							class="h-4 w-4 text-ink-subtle"
							fill="none"
							stroke="currentColor"
							viewBox="0 0 24 24"
						>
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="2"
								d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"
							></path>
						</svg>
					</a>
				</div>
			</div>

			<div v-if="activity.commits && activity.commits.length > 0">
				<h4 class="mb-3 flex items-center text-sm font-semibold text-ink-muted">
					<svg
						class="mr-2 h-5 w-5"
						fill="none"
						stroke="currentColor"
						viewBox="0 0 24 24"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							stroke-width="2"
							d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"
						></path>
					</svg>
					Commits ({{ activity.commits.length }})
				</h4>
				<div class="space-y-2">
					<a
						v-for="commit in activity.commits"
						:key="commit.id"
						:href="commit.url"
						target="_blank"
						class="block rounded-lg border border-line bg-surface p-3 transition hover:border-line-strong hover:bg-surface-hover"
					>
						<div class="flex items-start justify-between">
							<div class="flex-1">
								<p class="text-sm text-ink">
									{{ commit.message.split('\n')[0] }}
								</p>
								<div
									class="space-x-3 mt-1 flex items-center text-xs text-ink-subtle"
								>
									<span class="font-mono">{{ commit.short_sha }}</span>
									<span>{{ commit.author_name }}</span>
									<span>{{ formatDate(commit.committed_at) }}</span>
								</div>
							</div>
							<svg
								class="ml-2 h-4 w-4 shrink-0 text-ink-subtle"
								fill="none"
								stroke="currentColor"
								viewBox="0 0 24 24"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									stroke-width="2"
									d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"
								></path>
							</svg>
						</div>
					</a>
				</div>
			</div>

			<div v-if="activity.pull_requests && activity.pull_requests.length > 0">
				<h4 class="mb-3 flex items-center text-sm font-semibold text-ink-muted">
					<svg
						class="mr-2 h-5 w-5"
						fill="none"
						stroke="currentColor"
						viewBox="0 0 24 24"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							stroke-width="2"
							d="M8 7h12m0 0l-4-4m4 4l-4 4m0 6H4m0 0l4 4m-4-4l4-4"
						></path>
					</svg>
					Pull Requests ({{ activity.pull_requests.length }})
				</h4>
				<div class="space-y-2">
					<a
						v-for="pr in activity.pull_requests"
						:key="pr.id"
						:href="pr.url"
						target="_blank"
						class="block rounded-lg border border-line bg-surface p-3 transition hover:border-line-strong hover:bg-surface-hover"
					>
						<div class="flex items-start justify-between">
							<div class="flex-1">
								<div class="flex items-center">
									<span
										:class="[
											'mr-2 rounded px-2 py-1 text-xs font-semibold',
											pr.state === 'open'
												? 'bg-status-done-bg text-status-done-fg'
												: pr.state === 'merged'
													? 'bg-status-testing-bg text-status-testing-fg'
													: 'bg-status-fix-bg text-status-fix-fg',
										]"
									>
										{{ pr.state }}
									</span>
									<span class="font-semibold text-ink-muted"
										>#{{ pr.number }}</span
									>
								</div>
								<p class="mt-1 text-sm text-ink">
									{{ pr.title }}
								</p>
								<div
									class="space-x-3 mt-1 flex items-center text-xs text-ink-subtle"
								>
									<span>{{ pr.source_branch }} → {{ pr.target_branch }}</span>
									<span>by {{ pr.author_username }}</span>
								</div>
							</div>
							<svg
								class="ml-2 h-4 w-4 shrink-0 text-ink-subtle"
								fill="none"
								stroke="currentColor"
								viewBox="0 0 24 24"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									stroke-width="2"
									d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"
								></path>
							</svg>
						</div>
					</a>
				</div>
			</div>
		</div>

		<div
			v-if="error"
			class="mt-4 rounded-lg border border-status-fix bg-status-fix-bg p-4"
		>
			<p class="text-sm text-status-fix-fg">{{ error }}</p>
		</div>
	</div>
</template>

<script>
	import { getTaskGitActivity } from '@/actions/tmgr/github';

	export default {
		name: 'TaskGitActivity',
		props: {
			taskId: {
				type: [Number, String],
				required: true,
			},
			categoryCode: {
				type: String,
				default: 'TASK',
			},
		},
		data() {
			return {
				activity: {
					commits: [],
					branches: [],
					pull_requests: [],
				},
				loading: true,
				error: null,
			};
		},
		computed: {
			hasActivity() {
				return (
					this.activity.commits.length > 0 ||
					this.activity.branches.length > 0 ||
					this.activity.pull_requests.length > 0
				);
			},
		},
		async mounted() {
			await this.loadActivity();
		},
		watch: {
			hasActivity: {
				handler(newVal) {
					const count =
						(this.activity.commits?.length || 0) +
						(this.activity.branches?.length || 0) +
						(this.activity.pull_requests?.length || 0);
					this.$emit('update:count', count);
				},
				immediate: true,
			},
		},
		methods: {
			async loadActivity() {
				try {
					this.loading = true;
					this.error = null;
					this.activity = await getTaskGitActivity(this.taskId);
				} catch (e) {
					console.error('Failed to load git activity:', e);
					if (e.response?.status !== 404) {
						this.error = 'Failed to load git activity';
					}
				} finally {
					this.loading = false;
				}
			},
			formatDate(dateString) {
				const date = new Date(dateString);
				const now = new Date();
				const diffMs = now - date;
				const diffMins = Math.floor(diffMs / 60000);
				const diffHours = Math.floor(diffMs / 3600000);
				const diffDays = Math.floor(diffMs / 86400000);

				if (diffMins < 1) return 'just now';
				if (diffMins < 60) return `${diffMins}m ago`;
				if (diffHours < 24) return `${diffHours}h ago`;
				if (diffDays < 7) return `${diffDays}d ago`;

				return date.toLocaleDateString();
			},
		},
	};
</script>
