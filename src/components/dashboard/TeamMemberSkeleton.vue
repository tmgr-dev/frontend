<template>
  <div class="p-3 rounded-lg border border-gray-200 dark:border-gray-700">
    <div class="flex items-center justify-between">
      <div class="flex items-center space-x-3">
        <!-- Member Avatar Skeleton -->
        <div class="relative">
          <Skeleton class="w-10 h-10 rounded-full" />
          <Skeleton class="absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full" />
        </div>
        
        <!-- Member Info Skeleton -->
        <div class="flex-1 min-w-0 space-y-1">
          <Skeleton class="h-4 w-24" />
          <Skeleton class="h-3 w-32" />
        </div>
      </div>
      
      <!-- Timer Duration Skeleton -->
      <div class="text-right space-y-1">
        <Skeleton class="h-3 w-12" />
        <Skeleton class="h-3 w-16" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import Skeleton from '@/components/ui/skeleton/Skeleton.vue';
</script>