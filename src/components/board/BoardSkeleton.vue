<script setup lang="ts">
import { Skeleton } from '@/components/ui/skeleton';

interface Props {
  columnsCount?: number;
}

const props = withDefaults(defineProps<Props>(), {
  columnsCount: 4
});
</script>

<template>
  <div class="flex gap-4 overflow-x-auto pb-4 min-h-96">
    <div
      v-for="n in columnsCount"
      :key="n"
      class="w-72 flex-shrink-0 rounded-lg bg-gray-100 dark:bg-gray-800 p-4"
    >
      <div class="mb-4 flex items-center justify-between">
        <div class="flex items-center gap-2">
          <Skeleton class="h-3 w-3 rounded-full" />
          <Skeleton class="h-4 w-20" />
        </div>
        <Skeleton class="h-5 w-6 rounded-full" />
      </div>

      <div class="space-y-3">
        <div
          v-for="card in (n % 2 === 0 ? 2 : 3)"
          :key="card"
          class="rounded-lg bg-white dark:bg-gray-700 p-4 shadow-sm"
        >
          <Skeleton class="mb-2 h-4 w-3/4" />
          <Skeleton class="h-3 w-1/2" />
          <div class="mt-3 flex items-center gap-2">
            <Skeleton class="h-6 w-6 rounded-full" />
            <Skeleton class="h-3 w-16" />
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
