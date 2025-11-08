<template>
  <div class="p-3 rounded-lg border border-gray-200 dark:border-gray-700">
    <div class="flex items-start space-x-3">
      <!-- Activity Icon Skeleton -->
      <Skeleton class="w-8 h-8 rounded-full" />
      
      <!-- Activity Content Skeleton -->
      <div class="flex-1 min-w-0 space-y-2">
        <div class="flex items-center justify-between">
          <div class="flex items-center space-x-2">
            <!-- User Avatar Skeleton -->
            <Skeleton class="w-6 h-6 rounded-full" />
            
            <!-- User Name Skeleton -->
            <Skeleton class="h-4 w-20" />
          </div>
          
          <!-- Timestamp Skeleton -->
          <Skeleton class="h-3 w-16" />
        </div>
        
        <!-- Activity Description Skeleton -->
        <Skeleton class="h-4 w-3/4" />
        
        <!-- Metadata Skeleton -->
        <div class="flex space-x-2">
          <Skeleton class="h-6 w-16 rounded-md" />
          <Skeleton class="h-6 w-20 rounded-md" />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import Skeleton from '@/components/ui/skeleton/Skeleton.vue';
</script>