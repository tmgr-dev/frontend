<template>
	<alert ref="alert" />
	<div id="q-app" class="text-tmgr-blue dark:text-tmgr-gray">
		<Slideout
			menu="#menu"
			panel="#panel"
			side="right"
			@on-translate="translateMenu"
			@on-beforeclose="menuIsActive = false"
			@on-beforeopen="menuIsActive = true"
		>
			<div id="menu" class="overflow-y-hidden" style="overflow-y: hidden">
				<div class="z-20 px-4 text-right">
					<navbar-menu />
					<account-dropdown class="flex justify-end" />
					<span
						class="absolute bottom-0 right-0 pr-4 pb-10 text-black dark:text-white"
					>
						<day-night-switch v-model="switchOn" />
					</span>
				</div>
			</div>

			<div id="panel">
				<q-scroll-area
					:style="{
						height: bodyHeight + 'px',
					}"
				>
					<transition mode="out-in" name="fade">
						<div>
							<Navbar
								v-if="$route.meta.navbarHidden"
								:menu-is-active="menuIsActive"
								:menu-position="translateMenuPosition"
							/>
						</div>
					</transition>

					<router-view :key="$route.path" v-slot="{ Component }">
						<transition
							:name="transitionName"
							mode="out-in"
							@before-leave="beforeLeave"
							@enter="enter"
							@after-enter="afterEnter"
						>
							<div>
								<component :is="Component" v-if="showComponent"></component>
							</div>
						</transition>
					</router-view>
				</q-scroll-area>
			</div>
		</Slideout>

		<div class="fixed left-0 bottom-0 z-10 ml-10 mb-10 mr-2">
			<span v-for="task in activeTasks" class="mb-5 inline-block">
				<transition mode="out-in" name="fade">
					<a
						v-if="task.id !== $store.getters.currentOpenedTaskId"
						:href="`/${task.id}/edit`"
						@click.prevent="$store.commit('currentTaskIdForModal', task.id)"
					>
						<span
							class="relative mr-5 inline-flex rounded-md bg-gray-200 p-2 shadow-sm transition-colors duration-300 dark:bg-gray-800"
						>
							<span class="absolute top-0 left-0 -mt-2 -ml-2 flex h-5 w-5">
								<span
									class="absolute inline-flex h-full w-full animate-ping rounded-full bg-blue-400 opacity-75"
								/>
								<span
									class="relative inline-flex h-5 w-5 rounded-full bg-green-500"
								/>
							</span>

							<span class="text-tmgr-blue dark:text-tmgr-gray">
								{{ task.title }}
							</span>
						</span>
					</a>
				</transition>
			</span>
		</div>

		<Transition name="bounce-right-fade">
			<modal
				name="Task"
				v-if="showTaskFormModalWindow"
				close-on-bg-click
				modal-class="w-11/12 h-full"
				@close="$store.dispatch('closeTaskModal')"
			>
				<template #modal-body>
					<task-form
						:is-modal="true"
						:modal-project-category-id="
							$store.getters.createTaskInProjectCategoryId
						"
						:modal-task-id="$store.getters.currentTaskIdForModal"
						:status-id="$store.getters.createTaskInStatusId"
						@close="$store.dispatch('closeTaskModal')"
					/>
				</template>
			</modal>
		</Transition>
	</div>
</template>

<script>
	import { defineComponent } from 'vue';

	import Navbar from 'src/components/general/Navbar.vue';
	import NavbarMenu from 'src/components/general/NavbarMenu.vue';
	import Slideout from 'src/components/Slideout.vue';
	import TaskForm from 'src/pages/TaskForm.vue';
	import store from 'src/store';
	import { getUserSettings } from 'src/actions/tmgr/user';
	import { getLaunchedTasks } from 'src/actions/tmgr/tasks';
	import { getWorkspaceStatuses } from 'src/actions/tmgr/workspaces';

	const DEFAULT_TRANSITION = 'fade';

	// TODO: solve slideout & horizontal scroll in the Board
	export default defineComponent({
		name: 'App',
		components: {
			TaskForm,
			Navbar,
			Slideout,
			NavbarMenu,
		},
		data() {
			return {
				prevHeight: 0,
				transitionName: DEFAULT_TRANSITION,
				showComponent: true,
				activeTasks: [],
				bodyOverflow: '',
				bodyHeight: 800,
				menuIsActive: false,
				translateMenuPosition: 0,
			};
		},
		computed: {
			navbarHidden() {
				return this.$route.name !== 'Index';
			},
			switchOn: {
				get() {
					return this.$store.getters.colorScheme === 'dark';
				},
				set(newValue) {
					this.$store.commit('colorScheme', newValue ? 'dark' : 'default');
				},
			},
			showTaskFormModalWindow() {
				return (
					this.$route.name !== 'TasksEdit' &&
					(this.$store.getters.currentTaskIdForModal ||
						this.$store.getters.showCreateTaskModal)
				);
			},
		},
		watch: {
			'$route.path'() {
				this.showComponent = false;
				setTimeout(() => (this.showComponent = true), 100);
			},
		},
		methods: {
			closeTaskModal() {
				this.$store.dispatch('closeTaskModal');
			},
			translateMenu(data) {
				this.translateMenuPosition = data;
			},
			beforeLeave(element) {
				this.prevHeight = getComputedStyle(element).height;
			},
			enter(element) {
				const { height } = getComputedStyle(element);

				element.style.height = this.prevHeight;

				setTimeout(() => {
					element.style.height = height;
				});
			},
			afterEnter(element) {
				element.style.height = 'auto';
			},
			async loadActiveTasks() {
				if (!this.$store.getters.user) return;

				this.activeTasks = await getLaunchedTasks();
			},
			minimize() {
				if (process.env.MODE === 'electron') {
					window.myWindowAPI.minimize();
				}
			},
			maximize() {
				if (process.env.MODE === 'electron') {
					window.myWindowAPI.toggleMaximize();
				}
			},
			closeApp() {
				if (process.env.MODE === 'electron') {
					window.myWindowAPI.close();
				}
			},
			initBodyHeight() {
				setTimeout(() => {
					try {
						this.bodyHeight = this.getBodyHeight();
					} catch (e) {
						setTimeout(() => (this.bodyHeight = this.getBodyHeight()), 1000);
					}
				}, 500);
			},
			getBodyHeight() {
				return (
					this.getOffsetHeightOfElement('body') +
					30 -
					this.getOffsetHeightOfElement('[role=toolbar]') -
					this.getOffsetHeightOfElement('nav')
				);
			},
			getOffsetHeightOfElement(selector) {
				const el = document.querySelector(selector);
				if (!el) {
					return 0;
				}
				return el.offsetHeight;
			},
		},
		async created() {
			if (store.state.user) {
				await Promise.all([getUserSettings(), getWorkspaceStatuses()]);
			}

			this.$router.beforeEach((to, from, next) => {
				this.$store.commit('currentOpenedTaskId', null);
				this.loadActiveTasks();
				let transitionName = to.meta.transitionName || from.meta.transitionName;

				if (transitionName === 'slide') {
					const toDepth = to.path.split('/').length;
					const fromDepth = from.path.split('/').length;
					transitionName = toDepth < fromDepth ? 'slide-right' : 'slide-left';
				}

				this.transitionName = transitionName || DEFAULT_TRANSITION;

				next();
			});

			if (!this.$store.getters.user) {
				return;
			}
			this.$store.getters.pusherBeamsClient.getUserId().then((userId) => {
				if (!userId) {
					return this.$store.commit('pusherBeamsUserId', userId);
				}
				userId = this.$store.getters.user.id.toString();
				this.$store.getters.pusherBeamsClient.start().then(() => {
					this.$store.getters.pusherBeamsClient
						.setUserId(userId, this.$store.getters.pusherTokenProvider)
						.then(() => {
							this.$store.commit('pusherBeamsUserId', userId);
						});
				});
			});
			this.loadActiveTasks();
			if (process.env.MODE === 'electron') {
				document.body.style.overflow = 'hidden';
			}
		},
		mounted() {
			this.initBodyHeight();
		},
	});
</script>

<style lang="scss" src="src/assets/styles/index.scss"></style>
