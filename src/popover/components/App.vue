<template>
	<div ref="root" class="popover-root">
		<div
			v-if="model"
			class="pop"
			role="menu"
			:aria-label="$t('popover.window.label')"
		>
			<div class="head">
				<div class="logo">t</div>
				<div class="ws">
					<b>{{ workspaceName }}</b>
					<span>{{ $t('popover.header.workspace') }}</span>
				</div>
				<button type="button" class="btn" data-nav @click="run('open')">
					{{ $t('popover.header.open') }}
				</button>
			</div>
			<div class="sep"></div>
			<div class="body">
				<div class="sec">
					<input
						v-model="quick"
						type="text"
						class="quick"
						data-nav
						:placeholder="$t('popover.quickAdd.placeholder')"
						:aria-label="$t('popover.quickAdd.label')"
						@keydown.enter.prevent="quickAdd"
					/>
				</div>

				<div class="sec top0">
					<RunningTimer
						v-for="task in model.running"
						:key="`${task.workspaceId}:${task.id}`"
						:task="task"
						:clock-offset="clockOffset"
						@action="run"
					/>
					<div v-if="!model.running.length" class="empty">
						<span>{{ $t('popover.running.empty') }}</span>
						<button
							v-if="resumeTarget"
							type="button"
							class="btn accent"
							data-nav
							@click="run('resume-last')"
						>
							▶ {{ $t('popover.running.resume', { target: resumeTarget }) }}
						</button>
					</div>
				</div>

				<div v-if="show('decisions')" class="sec top0">
					<div class="label">
						{{ $t('popover.decisions.title') }}
						<span v-if="pending.length" class="badge">{{
							pending.length
						}}</span>
					</div>
					<p v-if="!pending.length" class="quiet">
						{{ $t('popover.decisions.empty') }}
					</p>
					<DecisionCard
						v-for="card in pending"
						:key="card.id"
						:card="card"
						:expanded="card.id === expandedId"
						@toggle="expandDecision"
						@answer="answer(card, $event)"
					/>
				</div>

				<div v-if="show('missed')" class="sec top0">
					<div class="label">{{ $t('popover.missed.title') }}</div>
					<div v-for="item in model.missed" :key="item.id" class="missed">
						<b>{{ item.title }}</b>
						<span>{{ item.body }}</span>
					</div>
				</div>

				<template v-if="show('switch')">
					<div class="sep"></div>
					<div class="sec top0">
						<div class="label">{{ $t('popover.switch.title') }}</div>
						<button
							v-for="task in model.recent"
							:key="`${task.workspaceId}:${task.id}`"
							type="button"
							class="row"
							data-nav
							@click="run(actionIds.switch(task.workspaceId, task.id))"
						>
							<span class="c">{{ codeOf(task.title) }}</span>
							<span class="t">{{ stripTicket(task.title) }}</span>
							<span v-if="todayOf(task)" class="m">{{ todayOf(task) }}</span>
							<span class="play" aria-hidden="true"></span>
						</button>
					</div>
				</template>

				<template v-if="show('today')">
					<div class="sep"></div>
					<div class="sec top0">
						<div class="label">{{ $t('popover.today.title') }}</div>
						<div class="kv">
							<span>{{ $t('popover.today.tracked') }}</span>
							<span>{{ trackedText }}</span>
						</div>
						<button
							v-for="routine in todayList.routines"
							:key="`routine-${routine.id}`"
							type="button"
							class="row"
							data-nav
							:disabled="routine.done"
							@click="run(actionIds.routineDone(routine.id))"
						>
							<span class="chk">{{ routine.done ? '✓' : '○' }}</span>
							<span class="t">{{ routine.title }}</span>
						</button>
						<button
							v-for="deadline in todayList.deadlines"
							:key="`deadline-${deadline.workspaceId}:${deadline.id}`"
							type="button"
							class="row"
							data-nav
							@click="run(actionIds.open(deadline.workspaceId, deadline.id))"
						>
							<span class="c">{{ deadline.code ?? '' }}</span>
							<span class="t">{{ deadline.title }}</span>
						</button>
						<button
							type="button"
							class="row"
							data-nav
							@click="run('open-routines')"
						>
							<span class="t">{{ $t('popover.today.openRoutines') }}</span>
							<span v-if="todayList.hidden" class="m">{{
								$t('popover.today.more', { count: todayList.hidden })
							}}</span>
							<span class="chev">›</span>
						</button>
					</div>
				</template>

				<template v-if="show('machines')">
					<div class="sep"></div>
					<div class="sec top0">
						<div class="label">{{ $t('popover.machines.title') }}</div>
						<button
							v-for="machine in model.extras.machines ?? []"
							:key="machine.id"
							type="button"
							class="row"
							data-nav
							@click="run(machine.local ? 'open-runner' : 'open-usage')"
						>
							<span class="state" :class="machine.state"></span>
							<span class="t">{{ machine.label }}</span>
							<span class="m">{{ machineMeta(machine) }}</span>
						</button>
					</div>
				</template>

				<div class="sep"></div>
				<div class="sec">
					<div class="dnd">
						<span>{{ $t('popover.dnd.title') }}</span>
						<div class="seg" role="group" :aria-label="$t('popover.dnd.title')">
							<button
								v-for="option in dndOptions"
								:key="option.id"
								type="button"
								data-nav
								:aria-pressed="model.dnd.option === option.id"
								@click="run(actionIds.dnd(option.id))"
							>
								{{ option.label }}
							</button>
						</div>
					</div>
					<button type="button" class="row" data-nav @click="run('shortcuts')">
						<span class="t">{{ $t('popover.shortcuts') }}</span>
					</button>
					<button
						type="button"
						class="row"
						data-nav
						role="switch"
						:aria-checked="model.autostart"
						@click="run('autostart')"
					>
						<span class="t">{{ $t('popover.autostart') }}</span>
						<span class="sw" :class="{ off: !model.autostart }"></span>
					</button>
					<button
						v-if="model.updaterEnabled"
						type="button"
						class="row"
						data-nav
						@click="run('tray_check_for_updates')"
					>
						<span class="t">{{ $t('popover.updates') }}</span>
					</button>
					<button type="button" class="row" data-nav @click="run('manager')">
						<span class="t">{{ $t('popover.manager') }}</span>
					</button>
				</div>

				<template v-if="show('ai')">
					<div class="sep"></div>
					<div class="sec">
						<div class="label">{{ $t('popover.usage.title') }}</div>
						<div class="ai">
							<div
								v-for="provider in usage.providers ?? []"
								:key="provider.tool"
								class="prov"
							>
								<div class="ph">
									<b>{{ $t(`popover.usage.${provider.tool}`) }}</b>
									<span v-if="provider.plan" class="plan">{{
										provider.plan
									}}</span>
									<span class="pt">
										<template v-if="provider.todayCost !== undefined">
											{{ moneyText(provider.todayCost) }}
										</template>
										<template v-if="provider.todayTokens !== undefined">
											· {{ tokensText(provider.todayTokens) }}
											{{ $t('popover.usage.tokens') }}
										</template>
									</span>
								</div>
								<UsageBar
									v-if="provider.fiveHour"
									:label="$t('popover.usage.fiveHour')"
									:bar="provider.fiveHour"
									:now="model.now"
								/>
								<UsageBar
									v-if="provider.week"
									:label="$t('popover.usage.week')"
									:bar="provider.week"
									:now="model.now"
								/>
							</div>
							<div class="tot">
								<span v-if="usage.todayCost !== undefined">
									{{ $t('popover.usage.today') }}
									<b>{{ moneyText(usage.todayCost) }}</b>
								</span>
								<span v-if="usage.todayTokens !== undefined">
									<b>{{ tokensText(usage.todayTokens) }}</b>
									{{ $t('popover.usage.tokens') }}
								</span>
								<span v-if="usage.weekCost !== undefined">
									{{ $t('popover.usage.weekTotal') }}
									<b>{{ moneyText(usage.weekCost) }}</b>
								</span>
								<span v-if="usage.runningCost !== undefined">
									{{ runningLabel }}
									<b>{{ moneyText(usage.runningCost) }}</b>
								</span>
							</div>
						</div>
						<button
							type="button"
							class="row"
							data-nav
							@click="run('open-usage')"
						>
							<span class="t">{{ $t('popover.usage.open') }}</span>
							<span class="chev">›</span>
						</button>
					</div>
				</template>

				<template v-if="show('plugins')">
					<div class="sep"></div>
					<div class="sec">
						<template
							v-for="section in model.pluginSections"
							:key="section.pluginName"
						>
							<button
								type="button"
								class="row plug"
								data-nav
								@click="togglePlugin(section.pluginName)"
							>
								<span class="pi">{{
									section.label.charAt(0).toUpperCase()
								}}</span>
								<span class="t">{{ section.label }}</span>
								<span class="chev">{{
									pluginOpen[section.pluginName] ? '▾' : '▸'
								}}</span>
							</button>
							<div v-if="pluginOpen[section.pluginName]" class="sub">
								<button
									v-for="item in section.items"
									:key="item.id"
									type="button"
									class="row"
									data-nav
									@click="run(actionIds.pluginItem(item.id))"
								>
									<span class="t">{{ item.title }}</span>
								</button>
							</div>
						</template>
					</div>
				</template>
			</div>
			<div class="foot">
				<span class="kbd">v{{ model.version }}</span>
				<button type="button" class="quit" data-nav @click="run('quit')">
					{{ $t('popover.footer.quit') }}
					<span class="kbd">{{ quitHint }}</span>
				</button>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import {
		decisionsAnswer,
		decisionsList,
		onDecisionsChanged,
	} from '@/decisions/api';
	import type { Answer, CardView } from '@/decisions/types';
	import {
		onPopoverModel,
		onPopoverShown,
		popoverAction,
		popoverQuickAdd,
		popoverResize,
		popoverState,
	} from '../api';
	import DecisionCard from './DecisionCard.vue';
	import RunningTimer from './RunningTimer.vue';
	import UsageBar from './UsageBar.vue';
	import {
		actionIds,
		moneyText,
		nextIndex,
		nextResize,
		pendingDecisions,
		stripTicket,
		ticketCode,
		durationText,
		todayEntries,
		tokensText,
		visibleSections,
		type PopoverSection,
		type PopoverUsage,
	} from '../model';
	import type { PopoverModel, PopoverTask } from '../types';

	const MAX_HEIGHT = 760;

	const NAV_SELECTOR = '[data-nav]:not([disabled])';

	export default defineComponent({
		components: { DecisionCard, RunningTimer, UsageBar },
		data() {
			return {
				model: null as PopoverModel | null,
				decisions: [] as CardView[],
				quick: '',
				openDecisionId: null as string | null,
				pluginOpen: {} as Record<string, boolean>,
				clockOffset: 0,
				lastHeight: 0,
				resizeObserver: null as ResizeObserver | null,
				unlisten: [] as Array<() => void>,
				actionIds,
			};
		},
		computed: {
			workspaceName(): string {
				return this.model?.extras.workspace?.name ?? 'TMGR';
			},
			sections(): PopoverSection[] {
				return this.model ? visibleSections(this.model) : [];
			},
			pending(): CardView[] {
				return pendingDecisions(this.decisions);
			},
			expandedId(): string | null {
				const open = this.pending.find((c) => c.id === this.openDecisionId);
				return open ? open.id : (this.pending[0]?.id ?? null);
			},
			resumeTarget(): string {
				const last = this.model?.recent[0];
				if (!last) return '';
				return ticketCode(last.title) ?? stripTicket(last.title);
			},
			runningLabel(): string {
				const first = this.model?.running[0];
				if (!first) return this.$t('popover.usage.running');
				return ticketCode(first.title) ?? this.$t('popover.usage.running');
			},
			todayList() {
				const today = this.model?.extras.today;
				return todayEntries(today?.routines ?? [], today?.deadlines ?? []);
			},
			trackedText(): string {
				return durationText(this.model?.extras.today?.trackedSeconds ?? 0);
			},
			usage(): PopoverUsage {
				return this.model?.extras.usage ?? {};
			},
			dndOptions() {
				return [
					{ id: 'off' as const, label: this.$t('popover.dnd.off') },
					{ id: '1h' as const, label: this.$t('popover.dnd.1h') },
					{ id: '3h' as const, label: this.$t('popover.dnd.3h') },
					{ id: 'tomorrow' as const, label: this.$t('popover.dnd.tomorrow') },
				];
			},
			quitHint(): string {
				return this.model?.platform === 'macos' ? '⌘Q' : 'Ctrl+Q';
			},
		},
		mounted() {
			window.addEventListener('keydown', this.onKey);
			window.addEventListener('focus', this.reload);
			const root = this.$refs.root as HTMLElement | undefined;
			if (root) {
				this.resizeObserver = new ResizeObserver(() => this.resize());
				this.resizeObserver.observe(root);
			}
			void Promise.all([
				onPopoverModel((model) => this.apply(model)),
				onPopoverShown(() => {
					this.lastHeight = 0;
					this.reload();
				}),
				onDecisionsChanged(() => void this.refreshDecisions()),
			]).then((fns) => {
				this.unlisten = fns;
			});
			this.reload();
		},
		beforeUnmount() {
			window.removeEventListener('keydown', this.onKey);
			window.removeEventListener('focus', this.reload);
			this.resizeObserver?.disconnect();
			this.unlisten.forEach((fn) => fn());
		},
		methods: {
			reload() {
				popoverState()
					.then((model) => this.apply(model))
					.catch(() => undefined);
			},
			apply(model: PopoverModel) {
				this.model = model;
				this.clockOffset = model.now - Date.now() / 1000;
				void this.$nextTick(() => this.resize());
				if (model.decisionsAvailable) {
					void this.refreshDecisions();
				} else {
					this.decisions = [];
				}
			},
			async refreshDecisions() {
				if (!this.model?.decisionsAvailable) return;
				try {
					this.decisions = await decisionsList();
				} catch {
					this.decisions = [];
				}
			},
			show(section: PopoverSection): boolean {
				return this.sections.includes(section);
			},
			run(id: string) {
				void popoverAction(id);
			},
			quickAdd() {
				const title = this.quick.trim();
				if (!title) return;
				popoverQuickAdd(title)
					.then(() => {
						this.quick = '';
						return popoverAction('close');
					})
					.catch(() => undefined);
			},
			async answer(card: CardView, choice: string) {
				const answer: Answer = {
					choice,
					line_comments: [],
					deferred: false,
					dismissed: false,
					seen_ms: card.updated_ms ?? card.created_ms,
				};
				await decisionsAnswer(card.id, answer).catch(() => undefined);
				await this.refreshDecisions();
			},
			expandDecision(id: string) {
				this.openDecisionId = id;
			},
			togglePlugin(name: string) {
				this.pluginOpen = {
					...this.pluginOpen,
					[name]: !this.pluginOpen[name],
				};
			},
			codeOf(title: string): string {
				return ticketCode(title) ?? '';
			},
			stripTicket,
			todayOf(task: PopoverTask): string | null {
				const seconds =
					this.model?.extras.todaySecondsByTask?.[
						`${task.workspaceId}:${task.id}`
					];
				return seconds === undefined ? null : durationText(seconds);
			},
			machineMeta(machine: { state: string; sessions: number | null }): string {
				const state = this.machineState(machine.state);
				return machine.sessions === null
					? state
					: `${this.$t('popover.machines.sessions', { count: machine.sessions })} · ${state}`;
			},
			machineState(state: string): string {
				switch (state) {
					case 'online':
						return this.$t('popover.machines.state.online');
					case 'busy':
						return this.$t('popover.machines.state.busy');
					default:
						return this.$t('popover.machines.state.offline');
				}
			},
			moneyText,
			tokensText,
			navItems(): HTMLElement[] {
				const root = this.$refs.root as HTMLElement | undefined;
				return root
					? Array.from(root.querySelectorAll<HTMLElement>(NAV_SELECTOR))
					: [];
			},
			onKey(event: KeyboardEvent) {
				if (event.key === 'Escape') {
					event.preventDefault();
					this.run('close');
					return;
				}
				if (
					(event.metaKey || event.ctrlKey) &&
					event.key.toLowerCase() === 'q'
				) {
					event.preventDefault();
					this.run('quit');
					return;
				}
				if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
					const items = this.navItems();
					const current = items.indexOf(document.activeElement as HTMLElement);
					const next = nextIndex(
						current,
						items.length,
						event.key === 'ArrowDown' ? 1 : -1,
					);
					if (next < 0) return;
					event.preventDefault();
					items[next].focus();
				}
			},
			resize() {
				const root = this.$refs.root as HTMLElement | undefined;
				if (!root) return;
				const height = nextResize(
					root.getBoundingClientRect().height,
					MAX_HEIGHT,
					this.lastHeight,
				);
				if (height === null) return;
				this.lastHeight = height;
				popoverResize(height).catch(() => undefined);
			},
		},
	});
</script>
