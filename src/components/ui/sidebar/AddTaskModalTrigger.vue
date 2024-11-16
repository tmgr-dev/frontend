<script setup lang="ts">
import type { HTMLAttributes } from 'vue'
import Button from '@/components/ui/button/Button.vue'
import { cn } from '@/utils'
import { FilePlus } from 'lucide-vue-next'

const props = defineProps<{
  class?: HTMLAttributes['class']
}>()


</script>

<template>
  <Button
    data-sidebar="trigger"
    variant="ghost"
    size="icon"
    :class="cn('h-7 w-7', props.class)"
    @click="$store.commit('setShowCreatingTaskModal')"
  >
    <FilePlus />
    <span class="sr-only">Open Add Task Modal</span>
  </Button>
</template>
