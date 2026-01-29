<script setup lang="ts">
import type { HTMLAttributes } from 'vue'
import { cn } from '@/utils'

interface SkeletonCardProps {
  class?: HTMLAttributes['class']
}

const props = defineProps<SkeletonCardProps>()
</script>

<template>
  <div :class="cn('h-32 w-full animate-pulse rounded-lg bg-muted', props.class)">
    <slot />
  </div>
</template>
