<template>
  <div class="heatmap-calendar w-full">
    <div class="mb-6">
      <div :class="['text-sm font-medium mb-1', textColorClass]">
        {{ data.total_contributions }} contributions in {{ currentYear }}
      </div>
      <div :class="['text-xs', textColorClass]">
        Current streak: {{ data.streak.current }} {{ data.streak.current === 1 ? 'day' : 'days' }} • 
        Longest: {{ data.streak.longest }} {{ data.streak.longest === 1 ? 'day' : 'days' }}
      </div>
    </div>

    <div class="w-full overflow-x-auto scroll-smooth">
      <div class="min-w-[600px]">
        <div class="relative">
          <div class="grid grid-cols-[auto_1fr] gap-4">
            <div class="flex flex-col justify-around" style="height: 105px;">
              <div :class="['text-[9px] leading-none', textColorClass]">Mon</div>
              <div :class="['text-[9px] leading-none', textColorClass]">Wed</div>
              <div :class="['text-[9px] leading-none', textColorClass]">Fri</div>
            </div>

            <div class="flex-1 min-w-0">
              <div class="flex justify-between mb-2 px-[1px]">
                <div 
                  v-for="month in monthLabels" 
                  :key="month.label"
                  :class="['text-[9px]', textColorClass]"
                >
                  {{ month.label }}
                </div>
              </div>

              <div class="grid gap-[2px]" :style="weekGridStyle">
                <div 
                  v-for="(week, weekIndex) in calendarGrid" 
                  :key="weekIndex"
                  class="grid gap-[2px]"
                  :style="dayGridStyle"
                >
                  <div
                    v-for="day in week"
                    :key="day.date"
                    :class="getSquareClass(day)"
                    class="rounded-[2px] transition-transform duration-200 hover:scale-110 cursor-pointer"
                    :style="squareStyle"
                    @mouseenter="handleSquareHover(day, $event)"
                    @mouseleave="handleSquareLeave"
                    :title="getTooltipText(day)"
                  />
                </div>
              </div>

              <div class="flex items-center justify-end gap-2 mt-3">
                <span :class="['text-[9px]', textColorClass]">Less</span>
                <div class="flex gap-[3px]">
                  <div :class="['w-[11px] h-[11px] rounded-[2px]', emptySquareClass]" />
                  <div :class="['w-[11px] h-[11px] rounded-[2px]', level1Class]" />
                  <div :class="['w-[11px] h-[11px] rounded-[2px]', level2Class]" />
                  <div :class="['w-[11px] h-[11px] rounded-[2px]', level3Class]" />
                  <div :class="['w-[11px] h-[11px] rounded-[2px]', level4Class]" />
                </div>
                <span :class="['text-[9px]', textColorClass]">More</span>
              </div>
            </div>
          </div>

          <div
            v-if="hoveredDate"
            class="absolute z-50 pointer-events-none px-2 py-1 rounded text-xs whitespace-nowrap"
            :class="theme === 'dark' ? 'bg-gray-900 text-white' : 'bg-gray-800 text-white'"
            :style="tooltipStyle"
          >
            {{ hoveredDate.count }} {{ hoveredDate.count === 1 ? 'contribution' : 'contributions' }} on {{ formatDate(hoveredDate.date) }}
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script>
import { defineComponent } from 'vue';

export default defineComponent({
  name: 'HeatmapCalendar',
  props: {
    data: {
      type: Object,
      required: true,
      validator: (value) => {
        return value.contributions && 
               Array.isArray(value.contributions) && 
               typeof value.total_contributions === 'number' &&
               value.streak &&
               typeof value.streak.current === 'number' &&
               typeof value.streak.longest === 'number';
      }
    },
    theme: {
      type: String,
      default: 'light',
      validator: (value) => ['light', 'dark'].includes(value)
    }
  },
  data() {
    return {
      hoveredDate: null,
      tooltipPosition: { x: 0, y: 0 }
    };
  },
  computed: {
    currentYear() {
      return new Date().getFullYear();
    },
    
    contributionMap() {
      const map = {};
      this.data.contributions.forEach(item => {
        map[item.date] = item.count;
      });
      return map;
    },
    
    calendarGrid() {
      const grid = [];
      const year = this.currentYear;
      const startDate = new Date(year, 0, 1);
      const endDate = new Date(year, 11, 31);
      
      let currentDate = new Date(startDate);
      const dayOfWeek = currentDate.getDay();
      const daysToMonday = dayOfWeek === 0 ? 6 : dayOfWeek - 1;
      currentDate.setDate(currentDate.getDate() - daysToMonday);
      
      let week = [];
      
      while (currentDate <= endDate || week.length > 0) {
        const dateStr = this.formatDateYMD(currentDate);
        const count = this.contributionMap[dateStr] || 0;
        const isInRange = currentDate >= startDate && currentDate <= endDate;
        
        week.push({
          date: dateStr,
          count: isInRange ? count : 0,
          isInRange: isInRange,
          dayOfWeek: currentDate.getDay()
        });
        
        if (week.length === 7) {
          grid.push(week);
          week = [];
        }
        
        currentDate.setDate(currentDate.getDate() + 1);
        
        if (currentDate > endDate && week.length === 0) break;
      }
      
      return grid;
    },
    
    monthLabels() {
      const labels = [];
      const year = this.currentYear;
      const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
      
      let lastMonth = -1;
      const monthFirstWeek = new Array(12).fill(-1);
      
      this.calendarGrid.forEach((week, weekIndex) => {
        const firstValidDay = week.find(day => day.isInRange);
        if (firstValidDay) {
          const date = new Date(firstValidDay.date);
          const month = date.getMonth();
          
          if (month !== lastMonth && monthFirstWeek[month] === -1) {
            monthFirstWeek[month] = weekIndex;
            lastMonth = month;
          }
        }
      });
      
      months.forEach((monthName, index) => {
        if (monthFirstWeek[index] !== -1) {
          labels.push({
            label: monthName,
            weekIndex: monthFirstWeek[index]
          });
        }
      });
      
      return labels;
    },
    
    weekGridStyle() {
      const weeks = this.calendarGrid.length;
      return {
        gridTemplateColumns: `repeat(${weeks}, 1fr)`,
        gridAutoFlow: 'column'
      };
    },
    
    dayGridStyle() {
      return {
        gridTemplateRows: 'repeat(7, 1fr)',
        gridAutoFlow: 'row'
      };
    },
    
    squareStyle() {
      return {
        width: '100%',
        aspectRatio: '1 / 1'
      };
    },
    
    textColorClass() {
      return this.theme === 'dark' ? 'text-[#7d8590]' : 'text-[#656d76]';
    },
    
    emptySquareClass() {
      return this.theme === 'dark' ? 'bg-[#161b22]' : 'bg-[#ebedf0]';
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
        top: `${this.tooltipPosition.y - 40}px`,
        transform: 'translateX(-50%)'
      };
    }
  },
  methods: {
    getIntensityLevel(count) {
      if (count === 0) return 0;
      if (count >= 1 && count <= 5) return 1;
      if (count >= 6 && count <= 15) return 2;
      if (count >= 16 && count <= 30) return 3;
      return 4;
    },
    
    getSquareClass(day) {
      if (!day.isInRange) {
        return this.emptySquareClass;
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
    
    handleSquareHover(day, event) {
      if (!day.isInRange) return;
      
      const rect = event.target.getBoundingClientRect();
      const parentRect = event.target.closest('.heatmap-calendar').getBoundingClientRect();
      
      this.hoveredDate = day;
      this.tooltipPosition = {
        x: rect.left - parentRect.left + rect.width / 2,
        y: rect.top - parentRect.top
      };
    },
    
    handleSquareLeave() {
      this.hoveredDate = null;
    },
    
    getTooltipText(day) {
      if (!day.isInRange) return '';
      return `${day.count} ${day.count === 1 ? 'contribution' : 'contributions'} on ${this.formatDate(day.date)}`;
    },
    
    formatDate(dateStr) {
      const date = new Date(dateStr);
      const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
      return `${months[date.getMonth()]} ${date.getDate()}, ${date.getFullYear()}`;
    },
    
    formatDateYMD(date) {
      const year = date.getFullYear();
      const month = String(date.getMonth() + 1).padStart(2, '0');
      const day = String(date.getDate()).padStart(2, '0');
      return `${year}-${month}-${day}`;
    }
  }
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

