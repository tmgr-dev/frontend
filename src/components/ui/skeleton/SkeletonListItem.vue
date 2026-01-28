<script setup lang="ts">
import type { HTMLAttributes } from 'vue'
import { cn } from '@/utils'
import Skeleton from './Skeleton.vue'

interface SkeletonListItemProps {
  class?: HTMLAttributes['class']
  showAvatar?: boolean
}

const props = withDefaults(defineProps<SkeletonListItemProps>(), {
  showAvatar: true
})
</script>

<template>
  <div :class="cn('flex items-center space-x-3 p-3', props.class)">
    <Skeleton v-if="showAvatar" class="h-10 w-10 shrink-0 rounded-full" />
    <div class="flex-1 space-y-2">
      <Skeleton class="h-4 w-3/4" />
      <Skeleton class="h-3 w-1/2" />
    </div>
  </div>
</template>
