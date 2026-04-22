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
  <div class="flex h-full min-h-[600px] gap-3 overflow-x-auto pb-4">
    <div
      v-for="n in columnsCount"
      :key="n"
      class="flex w-[300px] flex-shrink-0 flex-col px-1"
    >
      <!-- Status bar -->
      <Skeleton class="mb-2.5 h-[3px] rounded-full" />

      <!-- Column header -->
      <div class="mb-2 flex items-center gap-2 px-1">
        <Skeleton class="h-5 w-20 rounded-pill" />
        <Skeleton class="h-3 w-4" />
        <div class="ml-auto flex items-center gap-2">
          <Skeleton class="h-3 w-10" />
          <Skeleton class="h-5 w-5 rounded" />
        </div>
      </div>

      <!-- Cards -->
      <div class="mt-2 space-y-3 px-1">
        <div
          v-for="card in (n % 2 === 0 ? 2 : 3)"
          :key="card"
          class="relative rounded-card border border-line bg-surface p-[14px] shadow-tmgr-xs"
        >
          <!-- Left accent bar -->
          <span class="pointer-events-none absolute left-0 top-[10px] bottom-[10px] w-[3px] rounded-full bg-line-strong opacity-40" aria-hidden="true"></span>

          <!-- Header row: handle + timer pill + assignee -->
          <div class="mb-2.5 flex items-center gap-1.5">
            <Skeleton class="h-3.5 w-3.5" />
            <Skeleton class="h-[22px] w-12 rounded-pill" />
            <Skeleton class="ml-auto h-[22px] w-[22px] rounded-pill" />
          </div>

          <!-- Title (1-2 lines) -->
          <Skeleton class="mb-1.5 h-3.5 w-11/12" />
          <Skeleton v-if="card % 2 === 1" class="mb-3 h-3.5 w-2/3" />
          <div v-else class="mb-3"></div>

          <!-- Footer: category badge + date -->
          <div class="flex items-center justify-between">
            <Skeleton class="h-5 w-20 rounded-pill" />
            <Skeleton class="h-3 w-12" />
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
