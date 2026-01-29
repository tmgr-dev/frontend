<script setup lang="ts">
import { Skeleton } from '@/components/ui/skeleton';
</script>

<template>
  <!-- CHANGES: Added min-h-48 to match heatmap-container and prevent CLS -->
  <div class="w-full min-h-48">
    <div class="mb-6 space-y-2">
      <Skeleton class="h-4 w-48" />
      <Skeleton class="h-3 w-64" />
    </div>

    <div class="w-full overflow-x-auto">
      <div class="min-w-[600px]">
        <div class="relative">
          <div class="grid grid-cols-[auto_1fr] gap-4">
            <div class="flex flex-col justify-around" style="height: 105px;">
              <Skeleton class="h-2 w-6" />
              <Skeleton class="h-2 w-6" />
              <Skeleton class="h-2 w-6" />
            </div>

            <div class="flex-1 min-w-0">
              <div class="flex justify-between mb-2 px-[1px]">
                <Skeleton v-for="n in 12" :key="n" class="h-2 w-6" />
              </div>

              <div class="flex gap-[2px]">
                <div v-for="week in 53" :key="week" class="flex flex-col gap-[2px]">
                  <Skeleton
                    v-for="day in 7"
                    :key="day"
                    class="w-[11px] h-[11px] rounded-[2px]"
                  />
                </div>
              </div>
            </div>
          </div>
        </div>

        <div class="flex items-center justify-end gap-2 mt-4">
          <Skeleton class="h-3 w-8" />
          <div class="flex gap-[2px]">
            <Skeleton v-for="n in 5" :key="n" class="w-[11px] h-[11px] rounded-[2px]" />
          </div>
          <Skeleton class="h-3 w-8" />
        </div>
      </div>
    </div>
  </div>
</template>
