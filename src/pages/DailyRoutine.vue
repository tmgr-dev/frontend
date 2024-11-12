<template>
	<BaseLayout no-copyright>
		<template #body>
			<!-- Header Section with Input and Stats -->
			<div class="w-full items-center justify-between pt-6 md:flex md:py-4">
				<!-- Input Form -->
				<div class="rounded-xl p-2 dark:bg-gray-900 md:w-4/6">
					<div
						class="flex items-center justify-between rounded-xl border px-2 dark:border-0 dark:bg-gray-800"
					>
						<TextField
							class="w-5/6 p-2"
							v-model="form.title"
							@keyup.enter="createTask"
							placeholder="Enter your routine (enter to add new task)"
						/>
						<Button
							class="my-1 ml-2 w-32 rounded-xl px-2 py-2 text-sm text-white dark:bg-gray-800 dark:hover:bg-gray-700"
							@click="createTask"
						>
							<span class="relative">Add Task</span>
						</Button>
					</div>
				</div>

				<!-- Stats Section -->
				<ul class="m-4 flex justify-between md:mx-2">
					<li
						class="mx-2 flex w-28 flex-col rounded-xl border p-2 dark:border-0 dark:bg-gray-800"
					>
						<span>{{ tasks.length }}</span>
						<span class="text-gray-500">routines</span>
					</li>
					<li
						class="mx-2 flex w-28 flex-col rounded-xl border p-2 dark:border-0 dark:bg-gray-800"
					>
						<span>{{ completedCount }}</span>
						<span class="text-gray-500">completed</span>
					</li>
					<li
						class="mx-2 flex w-28 flex-col rounded-xl border p-2 dark:border-0 dark:bg-gray-800"
					>
						<span>{{ archivedTasksCount }}</span>
						<span class="text-gray-500">archived</span>
					</li>
				</ul>
			</div>

			<!-- Centered Trash Bin with Animation Container -->
			<div class="fixed bottom-8 right-8 z-50">
				<div
					ref="trashBinRef"
					class="trash-bin h-12 w-12 text-gray-600 dark:text-gray-400"
				>
					<!-- Trash Bin SVG -->
					<svg
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						stroke-linecap="round"
						stroke-linejoin="round"
						class="trash-icon"
					>
						<path class="trash-lid" d="M3 6h18" />
						<path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
						<path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
					</svg>
				</div>
			</div>

			<!-- Task List with Animation -->
			<div class="relative">
				<TransitionGroup
					name="task-list"
					tag="ul"
					class="w-full pt-4"
					:css="false"
					@before-leave="onBeforeLeave"
					@leave="onLeave"
				>
					<li
						v-for="task in tasks"
						:key="task.id"
						:data-task-id="task.id"
						class="task-item relative m-2 rounded-xl border px-5 py-3.5 dark:border-0 dark:bg-gray-800"
						:style="{
							backgroundColor: task.isAnimating ? 'transparent' : undefined,
						}"
					>
						<!-- Task Content -->
						<div
							class="task-content flex w-full items-center justify-between"
							:class="{ 'opacity-0': task.isAnimating }"
						>
							<!-- Left Side: Checkbox and Title -->
							<div class="flex items-center">
								<button
									@click="completeTask(task)"
									class="mr-3 rounded-full p-1 transition-colors hover:bg-gray-200 dark:hover:bg-gray-700"
									:class="{ 'text-green-500': task.status === 'completed' }"
								>
									<svg
										xmlns="http://www.w3.org/2000/svg"
										width="20"
										height="20"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
									>
										<path d="M20 6 9 17l-5-5" />
									</svg>
								</button>
								<span :class="{ 'line-through': task.status === 'completed' }">
									{{ task.title }}
								</span>
							</div>

							<!-- Right Side: Action Buttons -->
							<div class="flex items-center">
								<button
									v-if="task.status === 'completed'"
									@click="archiveTask(task)"
									class="mr-2 rounded-full p-1.5 text-gray-500 transition-colors hover:bg-gray-200 hover:text-blue-500 dark:hover:bg-gray-700"
								>
									<svg
										xmlns="http://www.w3.org/2000/svg"
										width="18"
										height="18"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
									>
										<path
											d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
										/>
										<line x1="12" y1="11" x2="12" y2="17" />
										<line x1="9" y1="14" x2="15" y2="14" />
									</svg>
								</button>
								<button
									@click="deleteTask(task)"
									class="rounded-full p-1.5 text-gray-500 transition-colors hover:bg-gray-200 hover:text-red-500 dark:hover:bg-gray-700"
								>
									<svg
										xmlns="http://www.w3.org/2000/svg"
										width="18"
										height="18"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
									>
										<path d="M3 6h18" />
										<path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
										<path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
										<line x1="10" y1="11" x2="10" y2="17" />
										<line x1="14" y1="11" x2="14" y2="17" />
									</svg>
								</button>
							</div>
						</div>

						<!-- Crumpled Paper SVG for Animation -->
						<div
							v-show="task.isAnimating"
							class="paper-animation z-max absolute left-0 top-0 h-full w-full"
						>
							<svg
								class="crumpled-paper"
								width="200px"
								height="200px"
								viewBox="0 0 15 15"
								fill="none"
								xmlns="http://www.w3.org/2000/svg"
							>
								<path
									fill-rule="evenodd"
									clip-rule="evenodd"
									d="M4.60913 0.0634287C4.39082 0.0088505 4.16575 0.12393 4.08218 0.332867L3.1538 2.6538L0.832866 3.58218C0.702884 3.63417 0.604504 3.7437 0.566705 3.87849C0.528906 4.01329 0.555994 4.158 0.639992 4.26999L2.01148 6.09864L1.06343 9.89085C1.00944 10.1068 1.12145 10.3298 1.32691 10.4154L4.20115 11.613L5.62557 13.7496C5.73412 13.9124 5.93545 13.9864 6.12362 13.9327L9.62362 12.9327C9.62988 12.9309 9.63611 12.929 9.64229 12.9269L12.6423 11.9269C12.7923 11.8769 12.905 11.7519 12.9393 11.5976L13.9393 7.09761C13.9776 6.92506 13.9114 6.74605 13.77 6.63999L11.95 5.27499V2.99999C11.95 2.82955 11.8537 2.67373 11.7012 2.5975L8.70124 1.0975C8.67187 1.08282 8.64098 1.07139 8.60913 1.06343L4.60913 0.0634287ZM11.4323 6.01173L12.7748 7.01858L10.2119 9.15429C10.1476 9.20786 10.0995 9.2783 10.0731 9.35769L9.25382 11.8155L7.73849 10.8684C7.52774 10.7367 7.25011 10.8007 7.11839 11.0115C6.98667 11.2222 7.05074 11.4999 7.26149 11.6316L8.40341 12.3453L6.19221 12.9771L4.87441 11.0004C4.82513 10.9265 4.75508 10.8688 4.67307 10.8346L2.03046 9.73352L2.85134 6.44999H4.99999C5.24852 6.44999 5.44999 6.24852 5.44999 5.99999C5.44999 5.75146 5.24852 5.54999 4.99999 5.54999H2.72499L1.7123 4.19974L3.51407 3.47903L6.35769 4.4269C6.53655 4.48652 6.73361 4.42832 6.85138 4.28111L8.62413 2.06518L11.05 3.27811V5.19533L8.83287 6.08218C8.70996 6.13134 8.61494 6.23212 8.57308 6.35769L8.07308 7.85769C7.99449 8.09346 8.12191 8.34831 8.35769 8.4269C8.59346 8.50549 8.84831 8.37807 8.9269 8.14229L9.3609 6.84029L11.4323 6.01173ZM7.71052 1.76648L6.34462 3.47386L4.09505 2.724L4.77192 1.03183L7.71052 1.76648ZM10.2115 11.7885L12.116 11.1537L12.7745 8.19034L10.8864 9.76374L10.2115 11.7885Z"
									fill="#000000"
								/>
							</svg>
						</div>
					</li>
				</TransitionGroup>
			</div>
		</template>
	</BaseLayout>
</template>

<script setup lang="ts">
	import { computed, onMounted, ref } from 'vue';
	import TextField from '@/components/general/TextField.vue';
	import Button from '@/components/general/Button.vue';
	import {
		createDailyTask,
		getDailyTasks,
		deleteDailyTask,
		completeDailyTask,
		archiveDailyTask,
		getArchivedDailyTasksCount,
	} from '@/actions/tmgr/daily-tasks';
	import { Task } from '@/actions/tmgr/tasks';
	import gsap from 'gsap';

	// Types
	interface ExtendedTask extends Task {
		isAnimating?: boolean;
	}

	// Refs
	const tasks = ref<ExtendedTask[]>([]);
	const archivedTasksCount = ref<Number>(0);
	const form = ref<Task>({ title: '', status: 'backlog' } as Task);
	const trashBinRef = ref<HTMLElement | null>(null);

	// Computed
	const completedCount = computed(
		() => tasks.value.filter((task) => task.status === 'completed').length,
	);

	const archivedCount = computed(
		() => tasks.value.filter((task) => task.status === 'archived').length,
	);

	// Lifecycle
	onMounted(async () => {
		tasks.value = await getDailyTasks();
		archivedTasksCount.value = await getArchivedDailyTasksCount();
	});

	// Animation Functions
	function wiggleTrashBin() {
		if (!trashBinRef.value) return;

		// Animate trash bin
		gsap.to(trashBinRef.value, {
			rotation: -5,
			duration: 0.1,
			yoyo: true,
			repeat: 3,
			ease: 'elastic.out(1, 0.3)',
		});

		// Animate lid
		const lid = trashBinRef.value.querySelector('.trash-lid');
		if (lid) {
			gsap.to(lid, {
				rotation: -15,
				transformOrigin: 'right',
				duration: 0.2,
				yoyo: true,
				repeat: 1,
				ease: 'power2.inOut',
			});
		}
	}

	function createShrimpAnimation(taskEl: HTMLElement, onComplete: () => void) {
		const paperAnimation = taskEl.querySelector('.paper-animation');
		const crumpledPaper = taskEl.querySelector('.crumpled-paper');
		const trashBin = trashBinRef.value;

		if (!paperAnimation || !crumpledPaper || !trashBin) {
			onComplete();
			return;
		}

		// Get positions
		const taskRect = taskEl.getBoundingClientRect();
		const trashRect = trashBin.getBoundingClientRect();
		const windowCenter = window.innerWidth / 2;

		// Calculate positions for the shrimp-like movement
		const startX = taskRect.left;
		const startY = taskRect.top;
		const centerY = window.innerHeight / 2;
		const endX = trashRect.left + trashRect.width / 2 - taskRect.width / 2;
		const endY = trashRect.top;

		// Reset paper animation position
		gsap.set(paperAnimation, {
			display: 'block',
			scale: 1,
			rotation: 0,
			opacity: 1,
			x: 0,
			y: 0,
		});

		// Create animation timeline
		const tl = gsap.timeline({
			onComplete: () => {
				wiggleTrashBin();
				playTrashSound();
				setTimeout(onComplete, 300);
			},
		});

		// Shrimp-like animation sequence
		tl.to(paperAnimation, {
			duration: 0.3,
			scale: 0.8,
			rotation: -15,
			ease: 'power2.inOut',
		})
			.to(crumpledPaper, {
				duration: 0.2,
				scale: 0.9,
				opacity: 0.9,
				ease: 'power2.in',
			})
			.to(paperAnimation, {
				duration: 0.6,
				x: windowCenter - startX - taskRect.width / 2,
				y: centerY - startY,
				rotation: -180,
				scale: 0.6,
				ease: 'power1.inOut',
			})
			.to(paperAnimation, {
				duration: 0.4,
				x: endX - startX,
				y: endY - startY,
				rotation: '+=360',
				scale: 0.2,
				ease: 'power2.in',
				motionPath: {
					path: [
						{
							x: windowCenter - startX - taskRect.width / 2,
							y: centerY - startY,
						},
						{ x: endX - startX + 50, y: endY - startY - 100 },
						{ x: endX - startX, y: endY - startY },
					],
					curviness: 2,
				},
			})
			.to(paperAnimation, {
				duration: 0.2,
				scale: 0,
				opacity: 0,
				ease: 'power2.in',
			});

		// Animate crumple lines during the movement
		gsap.to(
			[
				taskEl.querySelector('.crumple-line-1'),
				taskEl.querySelector('.crumple-line-2'),
				taskEl.querySelector('.crumple-line-3'),
			],
			{
				duration: 0.8,
				opacity: 0.6,
				scale: 0.9,
				stagger: 0.1,
				repeat: -1,
				yoyo: true,
				ease: 'sine.inOut',
			},
		);
	}

	// Transition Group Handlers
	function onBeforeLeave(el: Element) {
		const taskId = parseInt(el.getAttribute('data-task-id') || '0');
		const task = tasks.value.find((t) => t.id === taskId);
		if (task) {
			task.isAnimating = true;
		}
	}

	function onLeave(el: Element, done: () => void) {
		createShrimpAnimation(el as HTMLElement, done);
	}

	// Task Management Functions
	async function createTask() {
		if (!form.value.title?.trim()) return;

		try {
			const newTask = await createDailyTask(form.value);
			tasks.value = [newTask, ...tasks.value];
			form.value = { title: '', status: 'backlog' } as Task;
		} catch (error) {
			console.error('Failed to create task:', error);
		}
	}

	async function completeTask(task: ExtendedTask) {
		try {
			const result = await completeDailyTask(task.id);
			const taskIndex = tasks.value.findIndex((t) => t.id === task.id);
			if (taskIndex !== -1) {
				tasks.value[taskIndex] = { ...result, isAnimating: false };
			}
		} catch (error) {
			console.error('Failed to complete task:', error);
		}
	}

	async function deleteTask(task: ExtendedTask) {
		try {
			const taskEl = document.querySelector(`[data-task-id="${task.id}"]`);
			if (!taskEl) return;

			// Set task as animating
			const taskIndex = tasks.value.findIndex((t) => t.id === task.id);
			if (taskIndex !== -1) {
				tasks.value[taskIndex] = {
					...tasks.value[taskIndex],
					isAnimating: true,
				};
			}

			// Create animation promise
			const animationPromise = new Promise<void>((resolve) => {
				createShrimpAnimation(taskEl as HTMLElement, () => {
					resolve();
					taskEl.remove();
				});
			});

			// Run API call and animation concurrently
			await Promise.all([deleteDailyTask(task.id), animationPromise]).then(
				() => {
					delete tasks.value[taskIndex];
				},
			);

			// Remove from local state
			tasks.value = tasks.value.filter((t) => t.id !== task.id);
		} catch (error) {
			console.error('Failed to delete task:', error);
		}
	}

	async function archiveTask(task: ExtendedTask) {
		try {
			const taskEl = document.querySelector(`[data-task-id="${task.id}"]`);
			if (!taskEl) return;

			const animationPromise = new Promise<void>((resolve) => {
				createShrimpAnimation(taskEl as HTMLElement, async () => {
					resolve();
					taskEl.remove();
					archivedTasksCount.value = await getArchivedDailyTasksCount();
				});
			});

			await Promise.all([archiveDailyTask(task.id), animationPromise]);

			tasks.value = tasks.value.filter((t) => t.id !== task.id);
		} catch (error) {
			console.error('Failed to archive task:', error);
		}
	}

	function playTrashSound() {
		// const audio = new Audio('/sounds/paper-crumple.mp3');
		// audio.volume = 0.5;
		// audio.play().catch(() => {
		// 	// Ignore audio play errors (e.g., if user hasn't interacted with page yet)
		// });
	}
</script>
<style>
	/* Animation Classes */
	.paper-animation {
		pointer-events: none;
		transform-origin: center center;
		will-change: transform;
	}

	.crumpled-paper {
		transform-origin: center;
		transition: all 0.3s ease;
	}

	/* Crumple Lines Animation */
	.crumple-line-1,
	.crumple-line-2,
	.crumple-line-3 {
		transform-origin: center;
		transition: all 0.3s ease;
	}

	/* Trash Bin Animation */
	.trash-bin {
		transform-origin: bottom center;
		transition: transform 0.3s ease;
	}

	.trash-lid {
		transform-origin: right;
		transition: transform 0.3s ease;
	}

	/* Task List Transitions */
	.task-list-move,
	.task-list-enter-active,
	.task-list-leave-active {
		transition: all 0.5s ease;
	}

	.task-list-enter-from,
	.task-list-leave-to {
		opacity: 0;
		transform: translateY(30px);
	}

	.task-list-leave-active {
		position: absolute;
	}

	/* Dark Mode Adjustments */
	.dark .crumpled-paper {
		color: #4a5568;
	}

	.dark .paper-animation {
		background-color: transparent;
	}

	/* Responsive Design */
	@media (max-width: 640px) {
		.trash-bin {
			bottom: 4rem;
			right: 1rem;
		}
	}
</style>
