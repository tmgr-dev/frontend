<script>
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'DemoTaskLayout',
		data() {
			return {
				newComment: '',
				task: {
					code: 'TMGR-T1042',
					title: 'Redesign onboarding flow for new workspaces',
					status: { name: 'In Progress', color: '#f59e0b' },
					assignee: { name: 'Anna Petrova', initials: 'AP', color: '#6366f1' },
					category: 'Product',
					priority: 'High',
					due: 'Jul 4, 2026',
					author: 'Dmitry K.',
					gitEvents: 3,
					links: 2,
					timer: { elapsed: '2h 14m', estimate: '4h', running: true },
					description: [
						'New workspaces drop users on an empty board with no guidance, so activation in the first session is low. We want a short, skippable onboarding that gets a user to their first created task fast.',
						'Scope: a 3-step inline checklist (create a status, add a task, invite a teammate), a dismissible hint card, and an empty-state illustration per view. Out of scope: video tours and email drip.',
					],
					checkpoints: [
						{ text: 'Audit current empty states (board / list / dashboard)', done: true },
						{ text: 'Design the 3-step inline checklist', done: true },
						{ text: 'Build dismissible hint card component', done: false },
						{ text: 'Track activation events', done: false },
					],
					comments: [
						{ author: 'Sergey V.', initials: 'SV', color: '#0ea5e9', time: '3 days ago', text: 'Can we make the checklist collapse once all steps are done? Otherwise it eats vertical space on the board.' },
						{ author: 'Anna Petrova', initials: 'AP', color: '#6366f1', time: '2 days ago', text: 'Yes — it collapses to a thin progress bar at 100% and can be reopened from the header.' },
						{ author: 'Dmitry K.', initials: 'DK', color: '#22c55e', time: '2 days ago', text: 'Please keep everything on semantic tokens so dark mode is free. No raw hex in the hint card.' },
						{ author: 'Maria L.', initials: 'ML', color: '#ec4899', time: '1 day ago', text: 'Copy draft for step 2 is ready in the doc. Tone is short + encouraging, matches the rest of the app.' },
						{ author: 'Sergey V.', initials: 'SV', color: '#0ea5e9', time: '6 hours ago', text: 'Started the hint card component, should have a PR up tomorrow. Will reuse the rounded-card surface.' },
						{ author: 'Anna Petrova', initials: 'AP', color: '#6366f1', time: '1 hour ago', text: 'Great. Last thing — let’s A/B the illustration vs. a plain headline before committing.' },
					],
				},
			};
		},
		computed: {
			variant() {
				return this.$route.meta?.demoVariant === 'b' ? 'b' : 'a';
			},
			doneCheckpoints() {
				return this.task.checkpoints.filter((c) => c.done).length;
			},
			checkpointPercent() {
				return Math.round(
					(this.doneCheckpoints / this.task.checkpoints.length) * 100,
				);
			},
		},
	});
</script>

<template>
	<div class="min-h-svh bg-surface-sunken font-display text-ink">
		<!-- Demo switcher bar -->
		<div
			class="sticky top-0 z-20 flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-line bg-surface px-4 py-3 md:px-6"
		>
			<span class="text-sm font-semibold">TM-162 · Task detail layout</span>
			<span class="hidden text-xs text-ink-subtle sm:inline"
				>full-width, two-column · prototype with mock data</span
			>
			<div
				class="ml-auto flex items-center gap-1 rounded-pill bg-surface-sunken p-1"
			>
				<router-link
					to="/demo/task-a"
					class="rounded-pill px-3 py-1 text-xs font-semibold transition"
					:class="
						variant === 'a'
							? 'bg-surface text-ink shadow-tmgr-xs'
							: 'text-ink-subtle hover:text-ink'
					"
					>Variant A</router-link
				>
				<router-link
					to="/demo/task-b"
					class="rounded-pill px-3 py-1 text-xs font-semibold transition"
					:class="
						variant === 'b'
							? 'bg-surface text-ink shadow-tmgr-xs'
							: 'text-ink-subtle hover:text-ink'
					"
					>Variant B</router-link
				>
			</div>
		</div>

		<div class="mx-auto w-full max-w-screen-2xl px-4 py-5 md:px-6 md:py-6">
			<!-- Top header row: status + code + actions -->
			<header class="mb-5 flex flex-wrap items-center gap-3">
				<span
					class="inline-flex items-center gap-1.5 rounded-pill bg-surface-sunken px-2.5 py-1 text-2xs font-bold uppercase tracking-wide text-ink-muted"
				>
					<span
						class="inline-block size-2 rounded-full"
						:style="{ backgroundColor: task.status.color }"
					/>
					{{ task.status.name }}
				</span>
				<span class="font-mono text-2xs text-ink-subtle">{{ task.code }}</span>
				<span
					class="rounded-pill bg-surface px-2 py-1 text-2xs font-semibold uppercase tracking-wide"
					:class="variant === 'a' ? 'text-brand' : 'text-ink-subtle'"
					>Variant {{ variant === 'a' ? 'A' : 'B' }}</span
				>
				<div class="ml-auto flex items-center gap-2">
					<button
						class="flex h-8 items-center gap-1.5 rounded-pill border border-line bg-surface px-3 text-xs font-medium text-ink hover:bg-surface-hover"
					>
						<span class="material-icons" style="font-size: 16px">share</span>
						Share
					</button>
					<button
						class="flex h-8 items-center gap-1.5 rounded-pill bg-status-done px-3 text-xs font-semibold text-white hover:opacity-90"
					>
						<span class="material-icons" style="font-size: 16px">save</span>
						Save
					</button>
				</div>
			</header>

			<!-- Two-column layout -->
			<div class="flex flex-col gap-6 lg:flex-row lg:items-start">
				<!-- LEFT column -->
				<main class="flex min-w-0 flex-1 flex-col gap-6">
					<!-- Title (both variants) -->
					<h1 class="text-2xl font-semibold leading-snug tracking-tight md:text-3xl">
						{{ task.title }}
					</h1>

					<!-- Timer (Variant A only, left) -->
					<div
						v-if="variant === 'a'"
						class="flex items-center justify-between rounded-card border border-line bg-surface p-4"
					>
						<div class="flex items-center gap-3">
							<button
								class="flex size-10 items-center justify-center rounded-pill text-white"
								:class="task.timer.running ? 'bg-status-fix' : 'bg-brand'"
							>
								<span class="material-icons">
									{{ task.timer.running ? 'pause' : 'play_arrow' }}
								</span>
							</button>
							<div>
								<div class="text-xl font-semibold tabular-nums">
									{{ task.timer.elapsed }}
								</div>
								<div class="text-xs text-ink-subtle">
									of {{ task.timer.estimate }} estimate
								</div>
							</div>
						</div>
						<span
							class="rounded-pill bg-surface-sunken px-2.5 py-1 text-2xs font-semibold uppercase tracking-wide text-ink-subtle"
							>{{ task.timer.running ? 'Running' : 'Paused' }}</span
						>
					</div>

					<!-- Properties (Variant A only, left) -->
					<div
						v-if="variant === 'a'"
						class="grid gap-x-4 gap-y-3 text-sm"
						style="grid-template-columns: minmax(110px, max-content) minmax(0, 1fr)"
					>
						<div class="flex items-center gap-2 text-ink-subtle">
							<span class="material-icons" style="font-size: 16px">person</span
							><span>Assignee</span>
						</div>
						<div class="flex items-center gap-2">
							<span
								class="flex size-6 items-center justify-center rounded-full text-2xs font-bold text-white"
								:style="{ backgroundColor: task.assignee.color }"
								>{{ task.assignee.initials }}</span
							>
							<span>{{ task.assignee.name }}</span>
						</div>

						<div class="flex items-center gap-2 text-ink-subtle">
							<span class="material-icons" style="font-size: 16px">folder</span
							><span>Category</span>
						</div>
						<div>
							<span
								class="rounded-pill bg-surface px-2.5 py-1 text-xs font-medium ring-1 ring-line"
								>{{ task.category }}</span
							>
						</div>

						<div class="flex items-center gap-2 text-ink-subtle">
							<span class="material-icons" style="font-size: 16px">flag</span
							><span>Priority</span>
						</div>
						<div class="font-medium text-status-fix-fg">{{ task.priority }}</div>

						<div class="flex items-center gap-2 text-ink-subtle">
							<span class="material-icons" style="font-size: 16px">event</span
							><span>Due date</span>
						</div>
						<div>{{ task.due }}</div>

						<div class="flex items-center gap-2 text-ink-subtle">
							<span class="material-icons" style="font-size: 16px">code</span
							><span>Git activity</span>
						</div>
						<div class="text-ink-subtle">{{ task.gitEvents }} events</div>

						<div class="flex items-center gap-2 text-ink-subtle">
							<span class="material-icons" style="font-size: 16px">person_outline</span
							><span>Author</span>
						</div>
						<div>{{ task.author }}</div>
					</div>

					<!-- Description (both) -->
					<section class="flex flex-col gap-2">
						<div
							class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
						>
							Description
						</div>
						<div
							class="flex flex-col gap-3 rounded-card border border-line bg-surface p-4 text-sm leading-relaxed text-ink"
						>
							<p v-for="(p, i) in task.description" :key="i">{{ p }}</p>
						</div>
					</section>

					<!-- Checkpoints (both) -->
					<section class="flex flex-col gap-2">
						<div class="flex items-center justify-between">
							<span
								class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
								>Checkpoints</span
							>
							<span class="text-xs tabular-nums text-ink-subtle"
								>{{ doneCheckpoints }}/{{ task.checkpoints.length }} done</span
							>
						</div>
						<div class="h-1.5 overflow-hidden rounded-full bg-surface-sunken">
							<div
								class="h-full rounded-full transition-all"
								:class="checkpointPercent === 100 ? 'bg-status-done' : 'bg-brand'"
								:style="{ width: checkpointPercent + '%' }"
							/>
						</div>
						<ul
							class="mt-1 flex flex-col divide-y divide-line rounded-card border border-line bg-surface"
						>
							<li
								v-for="(c, i) in task.checkpoints"
								:key="i"
								class="flex items-center gap-3 px-4 py-2.5 text-sm"
							>
								<span
									class="material-icons"
									:class="c.done ? 'text-status-done' : 'text-ink-faint'"
									style="font-size: 18px"
									>{{ c.done ? 'check_circle' : 'radio_button_unchecked' }}</span
								>
								<span :class="c.done ? 'text-ink-subtle line-through' : 'text-ink'">{{
									c.text
								}}</span>
							</li>
						</ul>
					</section>
				</main>

				<!-- RIGHT rail -->
				<aside
					class="flex w-full shrink-0 flex-col gap-4 lg:sticky lg:top-[4.5rem] lg:max-h-[calc(100dvh-6rem)] lg:w-[380px] xl:w-[400px]"
				>
					<!-- Variant B: properties + timer on top of the rail -->
					<template v-if="variant === 'b'">
						<div class="rounded-card border border-line bg-surface p-4">
							<div
								class="mb-3 text-2xs font-bold uppercase tracking-wide text-ink-subtle"
							>
								Properties
							</div>
							<div class="flex flex-col gap-3 text-sm">
								<div class="flex items-center justify-between gap-3">
									<span class="text-ink-subtle">Assignee</span>
									<span class="flex items-center gap-2">
										<span
											class="flex size-6 items-center justify-center rounded-full text-2xs font-bold text-white"
											:style="{ backgroundColor: task.assignee.color }"
											>{{ task.assignee.initials }}</span
										>
										<span>{{ task.assignee.name }}</span>
									</span>
								</div>
								<div class="flex items-center justify-between gap-3">
									<span class="text-ink-subtle">Category</span>
									<span
										class="rounded-pill bg-surface-sunken px-2.5 py-1 text-xs font-medium"
										>{{ task.category }}</span
									>
								</div>
								<div class="flex items-center justify-between gap-3">
									<span class="text-ink-subtle">Priority</span>
									<span class="font-medium text-status-fix-fg">{{
										task.priority
									}}</span>
								</div>
								<div class="flex items-center justify-between gap-3">
									<span class="text-ink-subtle">Due date</span><span>{{ task.due }}</span>
								</div>
								<div class="flex items-center justify-between gap-3">
									<span class="text-ink-subtle">Git activity</span
									><span class="text-ink-subtle">{{ task.gitEvents }} events</span>
								</div>
								<div class="flex items-center justify-between gap-3">
									<span class="text-ink-subtle">Author</span><span>{{ task.author }}</span>
								</div>
							</div>
						</div>

						<div
							class="flex items-center justify-between rounded-card border border-line bg-surface p-4"
						>
							<div class="flex items-center gap-3">
								<button
									class="flex size-9 items-center justify-center rounded-pill text-white"
									:class="task.timer.running ? 'bg-status-fix' : 'bg-brand'"
								>
									<span class="material-icons" style="font-size: 20px">{{
										task.timer.running ? 'pause' : 'play_arrow'
									}}</span>
								</button>
								<div>
									<div class="text-lg font-semibold tabular-nums">
										{{ task.timer.elapsed }}
									</div>
									<div class="text-xs text-ink-subtle">
										of {{ task.timer.estimate }}
									</div>
								</div>
							</div>
							<span
								class="rounded-pill bg-surface-sunken px-2 py-1 text-2xs font-semibold uppercase tracking-wide text-ink-subtle"
								>{{ task.timer.running ? 'Running' : 'Paused' }}</span
							>
						</div>
					</template>

					<!-- Comments panel (both variants) -->
					<div
						class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-card border border-line bg-surface"
					>
						<div
							class="flex shrink-0 items-center justify-between border-b border-line px-4 py-3"
						>
							<span class="text-sm font-semibold">Comments</span>
							<span class="text-xs text-ink-subtle"
								>{{ task.comments.length }}</span
							>
						</div>
						<div class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-4 py-4">
							<div
								v-for="(c, i) in task.comments"
								:key="i"
								class="flex gap-3"
							>
								<span
									class="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full text-2xs font-bold text-white"
									:style="{ backgroundColor: c.color }"
									>{{ c.initials }}</span
								>
								<div class="min-w-0">
									<div class="flex items-baseline gap-2">
										<span class="text-sm font-semibold">{{ c.author }}</span>
										<span class="text-2xs text-ink-faint">{{ c.time }}</span>
									</div>
									<p class="text-sm leading-relaxed text-ink">{{ c.text }}</p>
								</div>
							</div>
						</div>
						<div class="shrink-0 border-t border-line p-3">
							<div
								class="flex items-center gap-2 rounded-pill border border-line bg-surface-sunken py-1 pl-4 pr-1.5 focus-within:border-line-strong"
							>
								<input
									v-model="newComment"
									placeholder="Write a comment…"
									class="min-w-0 flex-1 bg-transparent text-sm text-ink outline-none placeholder:text-ink-subtle"
								/>
								<button
									class="flex size-7 items-center justify-center rounded-pill bg-brand text-white hover:bg-brand-hover"
								>
									<span class="material-icons" style="font-size: 16px">send</span>
								</button>
							</div>
						</div>
					</div>
				</aside>
			</div>
		</div>
	</div>
</template>
