<template>
	<alert ref="alert" />

	<div
		id="q-app"
		class="font-sans text-tmgr-blue dark:text-tmgr-gray"
		:key="$store.getters.appRerender"
	>
		<transition mode="out-in" name="fade">
			<Navbar v-if="$route.meta.navbarHidden" />
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

		<ActiveTasks :tasks="activeTasks" />

		<Transition name="bounce-right-fade">
			<Modal
				v-if="showTaskFormModalWindow"
				close-on-bg-click
				modal-class="w-11/12 h-full"
				@close="$store.dispatch('closeTaskModal')"
			>
				<template #modal-body>
					<TaskForm
						:is-modal="true"
						:modal-project-category-id="
							$store.getters.createTaskInProjectCategoryId
						"
						:modal-task-id="$store.getters.currentTaskIdForModal"
						:status-id="$store.getters.createTaskInStatusId"
						@close="$store.dispatch('closeTaskModal')"
					/>
				</template>
			</Modal>
		</Transition>
	</div>
</template>

<script>
	import { defineComponent } from 'vue';
	import Navbar from 'src/components/general/Navbar.vue';
	import TaskForm from 'src/pages/TaskForm.vue';
	import store from 'src/store';
	import { getUserSettings } from 'src/actions/tmgr/user';
	import { getLaunchedTasks } from 'src/actions/tmgr/tasks';
	import { getWorkspaceStatuses } from 'src/actions/tmgr/workspaces';
	import Alert from 'src/components/general/Alert.vue';
	import Modal from 'src/components/Modal.vue';
	import ActiveTasks from 'src/components/ActiveTasks.vue';

	const DEFAULT_TRANSITION = 'fade';

	export default defineComponent({
		name: 'App',
		components: {
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
