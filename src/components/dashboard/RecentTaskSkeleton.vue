<template>
  <div class="p-3 rounded-lg border border-gray-200 dark:border-gray-700">
    <div class="flex items-start justify-between">
      <div class="flex-1 min-w-0 space-y-2">
        <!-- Task Title and Status -->
        <div class="flex items-center space-x-2">
          <Skeleton class="w-3 h-3 rounded-full" />
          <Skeleton class="h-4 w-48" />
        </div>
        
        <!-- Task Metadata -->
        <div class="flex items-center space-x-4">
          <div class="flex items-center space-x-1">
            <Skeleton class="w-2 h-2 rounded-full" />
            <Skeleton class="h-3 w-16" />
          </div>
          <div class="flex items-center space-x-1">
            <Skeleton class="w-3 h-3" />
            <Skeleton class="h-3 w-20" />
          </div>
          <div class="flex items-center space-x-1">
            <Skeleton class="w-3 h-3" />
            <Skeleton class="h-3 w-12" />
          </div>
          <div class="flex items-center space-x-1">
            <Skeleton class="w-3 h-3" />
            <Skeleton class="h-3 w-16" />
          </div>
        </div>
      </div>
      
      <!-- Actions -->
      <div class="flex items-center space-x-2 ml-4">
        <Skeleton class="w-8 h-8 rounded" />
        <Skeleton class="w-24 h-8 rounded" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import Skeleton from '@/components/ui/skeleton/Skeleton.vue';
</script>
</template>
</invoke>