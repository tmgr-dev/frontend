<template>
	<alert ref="alert" />

	<div
		id="q-app"
		class="font-sans text-tmgr-blue dark:text-tmgr-gray"
		:key="$store.state.appRerenderKey"
	>
		<transition mode="out-in" name="fade">
			<AppNavigation @new-task="handleNewTask">
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
			</AppNavigation>
		</transition>

		<ActiveTasks :tasks="activeTasks" />

		<Transition name="bounce-right-fade">
			<Modal
				v-if="showTaskFormModalWindow"
				modal-class="h-full w-full md:w-auto md:h-auto"
				close-on-bg-click
				@close="$store.commit('closeTaskModal')"
			>
				<template #modal-body>
					<NewForm :is-modal="true" @close="$store.commit('closeTaskModal')" />

					<!--					<TaskForm
						is-modal
						:modal-project-category-id="
							$store.state.createTaskInProjectCategoryId
						"
						:modal-task-id="$store.state.currentTaskIdForModal"
						:status-id="$store.state.taskStatusId"
						@close="$store.commit('closeTaskModal')"
					/>-->
				</template>
			</Modal>
		</Transition>
	</div>
</template>

<script>
	import { defineComponent } from 'vue';
	import Navbar from '@/components/general/Navbar.vue';
	import TaskForm from '@/pages/TaskForm.vue';
	import store from '@/store';
	import { getUserSettings } from '@/actions/tmgr/user';
	import { getLaunchedTasks } from '@/actions/tmgr/tasks';
	import { getWorkspaceStatuses } from '@/actions/tmgr/workspaces';
	import Alert from '@/components/general/Alert.vue';
	import Modal from '@/components/Modal.vue';
	import ActiveTasks from '@/components/ActiveTasks.vue';
	import NewForm from '@/pages/NewForm.vue';
	import AppNavigation from '@/AppNavigation.vue';

	const DEFAULT_TRANSITION = 'fade';

	export default defineComponent({
		name: 'App',
		components: {
			NewForm,
			AppNavigation,
			ActiveTasks,
			Modal,
			Alert,
			TaskForm,
			Navbar,
		},
		data() {
			return {
				prevHeight: 0,
				transitionName: DEFAULT_TRANSITION,
				showComponent: true,
				activeTasks: [],
				bodyOverflow: '',
				bodyHeight: 800,
			};
		},
		computed: {
			navbarHidden() {
				return this.$route.name !== 'Index';
			},
			switchOn: {
				get() {
					return this.$store.state.colorScheme === 'dark';
				},
				set(newValue) {
					this.$store.commit('setColorScheme', newValue ? 'dark' : 'default');
				},
			},
			showTaskFormModalWindow() {
				return (
					this.$route.name !== 'TasksEdit' &&
					(this.$store.state.currentTaskIdForModal ||
						this.$store.state.showCreatingTaskModal)
				);
			},
		},
		watch: {
			'$route.path'() {
				this.showComponent = false;
				setTimeout(() => (this.showComponent = true), 100);
			},
			'$store.state.reloadActiveTasksKey'() {
				this.loadActiveTasks();
			},
		},
		methods: {
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
				if (!this.$store.state.user) return;

				this.activeTasks = await getLaunchedTasks();
			},
			minimize() {
				if (import.meta.env.MODE === 'electron') {
					window.myWindowAPI.minimize();
				}
			},
			maximize() {
				if (import.meta.env.MODE === 'electron') {
					window.myWindowAPI.toggleMaximize();
				}
			},
			closeApp() {
				if (import.meta.env.MODE === 'electron') {
					window.myWindowAPI.close();
				}
			},
			handleNewTask() {
				console.log('open modal');
				this.$store.commit('setShowCreatingTaskModal');
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

			if (!this.$store.state.user) {
				return;
			}
			this.$store.getters.getPusherBeamsClient.getUserId().then((userId) => {
				if (!userId) {
					return this.$store.commit('setPusherBeamsUserId', userId);
				}
				userId = this.$store.state.user.id.toString();
				this.$store.getters.getPusherBeamsClient.start().then(() => {
					this.$store.getters.getPusherBeamsClient
						.setUserId(userId, this.$store.getters.getPusherTokenProvider)
						.then(() => {
							this.$store.commit('setPusherBeamsUserId', userId);
						});
				});
			});
			this.loadActiveTasks();
			if (import.meta.env.MODE === 'electron') {
				document.body.style.overflow = 'hidden';
			}
		},
		mounted() {
			this.initBodyHeight();
		},
	});
</script>

<style lang="scss" src="@/assets/styles/index.scss"></style>
