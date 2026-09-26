<template>
	<div
		class="min-h-screen bg-background text-foreground antialiased lg:grid lg:grid-cols-[minmax(0,1.05fr)_minmax(0,1fr)]"
	>
		<aside
			class="relative hidden flex-col overflow-hidden border-r border-sidebar-border bg-sidebar px-12 pb-10 pt-16 text-sidebar-foreground lg:flex xl:px-16"
		>
			<div class="auth-dots pointer-events-none absolute inset-0" />

			<div class="relative flex items-center gap-2.5">
				<AuthMark />
				<span class="text-[15px] font-semibold tracking-tight">TMGR</span>
			</div>

			<div class="relative my-auto max-w-[480px] py-12">
				<h2
					class="text-[44px] font-semibold leading-[1.08] tracking-[-0.03em] text-foreground xl:text-[52px]"
				>
					Plan the work.<br />Keep the hours.
				</h2>
				<p
					class="mt-5 max-w-[400px] text-[15px] leading-relaxed text-muted-foreground"
				>
					Tasks, boards and a timer on every task, in one workspace you can
					share with your team.
				</p>

				<div
					class="mt-10 max-w-[420px] rounded-xl border border-border bg-card p-1.5 text-card-foreground shadow-lg"
					aria-hidden="true"
				>
					<div
						v-for="task in sampleTasks"
						:key="task.title"
						class="flex items-center gap-3 rounded-lg px-3 py-2.5"
						:class="task.running && 'bg-accent text-accent-foreground'"
					>
						<span
							class="h-2 w-2 shrink-0 rounded-full"
							:class="[task.dot, task.running && 'animate-tmgr-pulse']"
						/>
						<span class="min-w-0 flex-1 truncate text-[13px]">
							{{ task.title }}
						</span>
						<span
							class="shrink-0 rounded-full bg-muted px-2 py-0.5 text-[11px] font-medium text-muted-foreground"
						>
							{{ task.status }}
						</span>
						<span
							class="w-[64px] shrink-0 text-right font-mono text-[12px] tabular-nums"
							:class="
								task.running ? 'text-foreground' : 'text-muted-foreground'
							"
						>
							{{ task.running ? runningTime : task.time }}
						</span>
					</div>
				</div>
			</div>

			<ul
				class="relative flex flex-wrap gap-x-6 gap-y-2 text-[13px] text-muted-foreground"
			>
				<li
					v-for="feature in features"
					:key="feature.label"
					class="flex items-center gap-2"
				>
					<component :is="feature.icon" class="h-4 w-4" />
					{{ feature.label }}
				</li>
			</ul>
		</aside>

		<main
			class="relative flex min-h-screen flex-col px-4 pb-8 pt-14 sm:px-8 lg:pt-16"
		>
			<ThemeSwitch class="absolute right-4 top-12 sm:right-8 lg:top-14" />
			<div class="flex items-center justify-center gap-2.5 lg:hidden">
				<AuthMark />
				<span class="text-[15px] font-semibold tracking-tight">TMGR</span>
			</div>

			<div class="flex flex-1 items-center justify-center py-8">
				<div class="w-full max-w-[400px]">
					<div
						class="rounded-xl border border-border bg-card p-6 text-card-foreground shadow-lg sm:p-8"
					>
						<div class="mb-6">
							<h1
								class="text-[22px] font-semibold leading-tight tracking-tight"
							>
								<slot name="title" />
							</h1>
							<p
								v-if="$slots.subtitle"
								class="mt-1.5 text-sm text-muted-foreground"
							>
								<slot name="subtitle" />
							</p>
						</div>

						<slot />
					</div>

					<div
						v-if="$slots.footer"
						class="mt-6 text-center text-sm text-muted-foreground"
					>
						<slot name="footer" />
					</div>
				</div>
			</div>
		</main>
	</div>
</template>

<script lang="ts">
	import ThemeSwitch from '@/components/auth/ThemeSwitch.vue';
	import logoUrl from '@/assets/img/simple-logo.svg';
	import { KanbanSquare, Timer, Users } from 'lucide-vue-next';
	import { defineComponent, h, onBeforeUnmount, onMounted, ref } from 'vue';

	const AuthMark = defineComponent({
		setup() {
			return () =>
				h(
					'span',
					{
						class:
							'flex h-8 w-8 items-center justify-center rounded-lg bg-primary',
					},
					[
						h('span', {
							class: 'h-[14px] w-[18px] bg-primary-foreground',
							style: {
								mask: `url("${logoUrl}") center / contain no-repeat`,
								WebkitMask: `url("${logoUrl}") center / contain no-repeat`,
							},
						}),
					],
				);
		},
	});

	export default defineComponent({
		name: 'AuthLayout',
		components: { AuthMark, ThemeSwitch },
		setup() {
			const sampleTasks = [
				{
					title: 'Ship onboarding checklist',
					status: 'Done',
					time: '2:15:40',
					dot: 'bg-status-done',
					running: false,
				},
				{
					title: 'Review sprint board',
					status: 'In progress',
					time: '',
					dot: 'bg-primary',
					running: true,
				},
				{
					title: 'Draft Q4 roadmap',
					status: 'To do',
					time: '0:00:00',
					dot: 'bg-status-todo',
					running: false,
				},
			];

			const features = [
				{ icon: KanbanSquare, label: 'Lists, boards, dashboard' },
				{ icon: Timer, label: 'Time tracking per task' },
				{ icon: Users, label: 'Shared workspaces' },
			];

			const elapsed = ref(5047);
			const format = (total: number) => {
				const hours = Math.floor(total / 3600);
				const minutes = String(Math.floor((total % 3600) / 60)).padStart(
					2,
					'0',
				);
				const seconds = String(total % 60).padStart(2, '0');
				return `${hours}:${minutes}:${seconds}`;
			};
			const runningTime = ref(format(elapsed.value));

			let interval: ReturnType<typeof setInterval> | undefined;
			onMounted(() => {
				interval = setInterval(() => {
					elapsed.value += 1;
					runningTime.value = format(elapsed.value);
				}, 1000);
			});
			onBeforeUnmount(() => clearInterval(interval));

			return { sampleTasks, features, runningTime };
		},
	});
</script>

<style scoped>
	.auth-dots {
		background-image: radial-gradient(hsl(var(--border)) 1px, transparent 1px);
		background-size: 20px 20px;
		mask-image: radial-gradient(
			ellipse 80% 70% at 70% 45%,
			#000 20%,
			transparent 75%
		);
	}
</style>
