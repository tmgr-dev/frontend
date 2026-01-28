<script setup lang="ts">
import type { HTMLAttributes } from 'vue'
import { computed } from 'vue'
import { cn } from '@/utils'
import Skeleton from './Skeleton.vue'

interface SkeletonAvatarProps {
  class?: HTMLAttributes['class']
  size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl'
}

const props = withDefaults(defineProps<SkeletonAvatarProps>(), {
  size: 'md'
})

const sizeClasses = {
  xs: 'h-5 w-5',
  sm: 'h-8 w-8',
  md: 'h-10 w-10',
  lg: 'h-12 w-12',
  xl: 'h-16 w-16'
}

const classes = computed(() => cn('shrink-0 rounded-full', sizeClasses[props.size], props.class))
</script>

<template>
  <Skeleton :class="classes" />
</template>
