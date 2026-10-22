<template>
	<div class="heatmap-calendar relative w-full">
		<div class="mb-6">
			<div
				data-testid="heatmap-total"
				:class="['mb-1 text-sm font-medium', textColorClass]"
			>
				{{ headline }}
			</div>
			<div data-testid="heatmap-streak" :class="['text-xs', textColorClass]">
				{{ streakText }}
			</div>
		</div>

		<div ref="scroller" class="w-full overflow-x-auto scroll-smooth">
			<div class="min-w-[600px]">
				<div>
					<div class="grid grid-cols-[auto_1fr] gap-4">
						<div class="flex flex-col justify-around" style="height: 105px">
							<div :class="['text-[9px] leading-none', textColorClass]">
								Mon
							</div>
							<div :class="['text-[9px] leading-none', textColorClass]">
								Wed
							</div>
							<div :class="['text-[9px] leading-none', textColorClass]">
								Fri
							</div>
						</div>

						<div class="min-w-0 flex-1">
							<div class="mb-2 grid gap-[2px] px-[2px]" :style="weekGridStyle">
								<div
									v-for="month in monthLabels"
									:key="month.key"
									:class="['whitespace-nowrap text-[9px]', textColorClass]"
									:style="{ gridColumnStart: month.column + 1 }"
								>
									{{ month.label }}
								</div>
							</div>

							<div class="grid gap-[2px] px-[2px]" :style="weekGridStyle">
								<div
									v-for="(week, weekIndex) in calendarGrid"
									:key="weekIndex"
									class="grid gap-[2px]"
									:style="dayGridStyle"
								>
									<div
										v-for="day in week"
										:key="day.key"
										:data-date="day.date || undefined"
										:class="[
											getSquareClass(day),
											day.date &&
												'cursor-pointer transition-transform duration-200 hover:scale-110',
											day.date &&
												day.date === selectedDate &&
												'relative z-10 ring-2 ring-ink ring-offset-1 ring-offset-surface',
										]"
										class="rounded-[2px]"
										:style="squareStyle"
										:role="day.date ? 'button' : undefined"
										:tabindex="day.date ? 0 : undefined"
										:aria-pressed="
											day.date ? String(day.date === selectedDate) : undefined
										"
										:aria-label="day.date ? getDayLabel(day) : undefined"
										@click="selectDay(day)"
										@keydown.enter="selectDay(day)"
										@keydown.space.prevent="selectDay(day)"
										@mouseenter="handleSquareHover(day, $event)"
										@mouseleave="handleSquareLeave"
										:title="getTooltipText(day)"
									/>
								</div>
							</div>

							<div class="mt-3 flex items-center justify-end gap-2">
								<span :class="['text-[9px]', textColorClass]">Less</span>
								<div class="flex gap-[3px]">
									<div
										:class="[
											'h-[11px] w-[11px] rounded-[2px]',
											emptySquareClass,
										]"
									/>
									<div
										:class="['h-[11px] w-[11px] rounded-[2px]', level1Class]"
									/>
									<div
										:class="['h-[11px] w-[11px] rounded-[2px]', level2Class]"
									/>
									<div
										:class="['h-[11px] w-[11px] rounded-[2px]', level3Class]"
									/>
									<div
										:class="['h-[11px] w-[11px] rounded-[2px]', level4Class]"
									/>
								</div>
								<span :class="['text-[9px]', textColorClass]">More</span>
							</div>
						</div>
					</div>
				</div>
			</div>
		</div>
		<div
			v-if="hoveredDate"
			ref="tooltip"
			class="pointer-events-none absolute z-50 whitespace-nowrap rounded px-2 py-1 text-xs"
			:class="
				theme === 'dark' ? 'bg-gray-900 text-white' : 'bg-gray-800 text-white'
			"
			:style="tooltipStyle"
		>
			{{ hoveredDate.count }}
			{{ hoveredDate.count === 1 ? 'contribution' : 'contributions' }} on
			{{ formatDate(hoveredDate.date) }}
		</div>
	</div>
</template>

<script>
	import {
		activityCountLabel,
		formatDayLabel,
	} from '@/utils/dashboard/activityDayFilter';
	import {
		buildHeatmapWeeks,
		heatmapHeadline,
		heatmapMonthLabels,
		heatmapStreakText,
		resolveHeatmapRange,
		utcToday,
	} from '@/utils/dashboard/heatmapRange';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'HeatmapCalendar',
		props: {
			data: {
				type: Object,
				required: true,
				validator: (value) => {
					return (
						value.contributions &&
						Array.isArray(value.contributions) &&
						typeof value.total_contributions === 'number' &&
						value.streak &&
						typeof value.streak.current === 'number' &&
						typeof value.streak.longest === 'number'
					);
				},
			},
			theme: {
				type: String,
				default: 'light',
				validator: (value) => ['light', 'dark'].includes(value),
			},
			selectedDate: {
				type: String,
				default: null,
			},
			year: {
				type: Number,
				default: null,
			},
		},
		emits: ['select'],
		watch: {
			year() {
				this.$nextTick(this.scrollToLatest);
			},
		},
		mounted() {
			this.scrollToLatest();
		},
		data() {
			return {
				hoveredDate: null,
				tooltipPosition: { x: 0, y: 0 },
			};
		},
		computed: {
			today() {
				return utcToday();
			},

			calendarGrid() {
				return buildHeatmapWeeks(
					resolveHeatmapRange(this.data, this.today),
					this.data.contributions,
				);
			},

			monthLabels() {
				return heatmapMonthLabels(this.calendarGrid);
			},

			headline() {
				return heatmapHeadline(this.data.total_contributions, this.year);
			},

			streakText() {
				return heatmapStreakText(this.data.streak, this.year, this.today);
			},

			weekGridStyle() {
				const weeks = Math.max(this.calendarGrid.length, 53);
				return {
					gridTemplateColumns: `repeat(${weeks}, minmax(0, 1fr))`,
					gridAutoFlow: 'column',
				};
			},

			dayGridStyle() {
				return {
					gridTemplateRows: 'repeat(7, 1fr)',
					gridAutoFlow: 'row',
				};
			},

			squareStyle() {
				return {
					width: '100%',
					aspectRatio: '1 / 1',
				};
			},

			textColorClass() {
				return this.theme === 'dark' ? 'text-[#7d8590]' : 'text-[#656d76]';
			},

			emptySquareClass() {
				return this.theme === 'dark' ? 'bg-line-strong' : 'bg-[#ebedf0]';
			},

			level1Class() {
				return this.theme === 'dark' ? 'bg-[#0e4429]' : 'bg-[#9be9a8]';
			},

			level2Class() {
				return this.theme === 'dark' ? 'bg-[#006d32]' : 'bg-[#40c463]';
			},

			level3Class() {
				return this.theme === 'dark' ? 'bg-[#26a641]' : 'bg-[#30a14e]';
			},

			level4Class() {
				return this.theme === 'dark' ? 'bg-[#39d353]' : 'bg-[#216e39]';
			},

			tooltipStyle() {
				return {
					left: `${this.tooltipPosition.x}px`,
					top: `${this.tooltipPosition.y}px`,
				};
			},
		},
		methods: {
			scrollToLatest() {
				const el = this.$refs.scroller;
				el?.scrollTo({ left: el.scrollWidth, behavior: 'instant' });
			},

			getIntensityLevel(count) {
				if (count === 0) return 0;
				if (count >= 1 && count <= 5) return 1;
				if (count >= 6 && count <= 15) return 2;
				if (count >= 16 && count <= 30) return 3;
				return 4;
			},

			getSquareClass(day) {
				if (!day.date) {
					return 'bg-transparent';
				}

				const level = this.getIntensityLevel(day.count);

				switch (level) {
					case 0:
						return this.emptySquareClass;
					case 1:
						return this.level1Class;
					case 2:
						return this.level2Class;
					case 3:
						return this.level3Class;
					case 4:
						return this.level4Class;
					default:
						return this.emptySquareClass;
				}
			},

			async handleSquareHover(day, event) {
				if (!day.date) return;

				const rect = event.target.getBoundingClientRect();
				const parentRect = event.target
					.closest('.heatmap-calendar')
					.getBoundingClientRect();

				this.hoveredDate = day;
				await this.$nextTick();
				const tooltip = this.$refs.tooltip;
				if (!tooltip || this.hoveredDate?.date !== day.date) return;

				const x =
					rect.left -
					parentRect.left +
					rect.width / 2 -
					tooltip.offsetWidth / 2;
				this.tooltipPosition = {
					x: Math.min(Math.max(x, 0), parentRect.width - tooltip.offsetWidth),
					y: rect.top - parentRect.top - tooltip.offsetHeight - 4,
				};
			},

			handleSquareLeave() {
				this.hoveredDate = null;
			},

			selectDay(day) {
				if (day.date) this.$emit('select', day.date);
			},

			getDayLabel(day) {
				return `${activityCountLabel(day.count)} on ${formatDayLabel(
					day.date,
				)}`;
			},

			getTooltipText(day) {
				if (!day.date) return '';
				return `${day.count} ${
					day.count === 1 ? 'contribution' : 'contributions'
				} on ${this.formatDate(day.date)}`;
			},

			formatDate(dateStr) {
				return formatDayLabel(dateStr);
			},
		},
	});
</script>

<style lang="scss" scoped>
	.heatmap-calendar {
		user-select: none;
	}

	.overflow-x-auto {
		&::-webkit-scrollbar {
			height: 8px;
		}

		&::-webkit-scrollbar-track {
			background: transparent;
		}

		&::-webkit-scrollbar-thumb {
			background: rgba(156, 163, 175, 0.5);
			border-radius: 4px;

			&:hover {
				background: rgba(156, 163, 175, 0.7);
			}
		}
	}
</style>
