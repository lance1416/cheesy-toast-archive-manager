<script setup lang="ts">
import { useArchiveStore, type FolderNode } from "../stores/archive";

defineProps<{
  node: FolderNode;
  depth: number;
}>();

const store = useArchiveStore();

const isSelected = (path: string) => store.currentDirectory === path;
</script>

<template>
  <div class="flex flex-col">
    <div
      class="flex items-center px-3 py-1.5 hover:bg-neutral-200 dark:hover:bg-neutral-800 transition-colors truncate"
      :class="{
        'bg-blue-100 dark:bg-blue-900/40 text-blue-700 dark:text-blue-400':
          isSelected(node.path),
      }"
      :style="{ paddingLeft: `${depth * 12 + 12}px` }"
      @click="store.navigateTo(node.path)"
    >
      <span class="mr-2 opacity-70">📁</span>
      <span class="truncate">{{ node.name }}</span>
    </div>

    <div v-if="Object.keys(node.children).length > 0">
      <Sidebar
        v-for="child in node.children"
        :key="child.path"
        :node="child"
        :depth="depth + 1"
      />
    </div>
  </div>
</template>
