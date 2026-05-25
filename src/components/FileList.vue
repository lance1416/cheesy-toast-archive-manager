<script setup lang="ts">
import { ref, computed } from "vue";
import { useVirtualizer } from "@tanstack/vue-virtual";
import { useArchiveStore, VfsNode } from "../stores/archive";

const store = useArchiveStore();
const scrollContainer = ref<HTMLDivElement | null>(null);

// Format bytes into human-readable strings
const formatSize = (bytes: number) => {
  if (bytes === 0) return "--";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
};

// Initialize the Virtualizer
const rowVirtualizer = useVirtualizer(
  computed(() => ({
    count: store.currentFolderNodes.length,
    getScrollElement: () => scrollContainer.value,
    estimateSize: () => 36, // Strict 36px height per row for native app density
    overscan: 5, // Render 5 extra items off-screen to prevent flickering
  })),
);

const handleDoubleClick = (node: VfsNode) => {
  if (node.is_dir) {
    store.navigateTo(node.path);
  } else {
    console.log("Extract/Open file:", node.path);
  }
};
</script>

<template>
  <div class="h-full w-full flex flex-col">
    <div
      class="flex px-4 py-2 border-b border-neutral-200 dark:border-neutral-800 text-xs font-semibold text-neutral-500 uppercase tracking-wider shrink-0 bg-neutral-50 dark:bg-neutral-900"
    >
      <div class="flex-1">Name</div>
      <div class="w-32 text-right">Size</div>
    </div>

    <div ref="scrollContainer" class="flex-1 overflow-auto outline-none">
      <div
        class="w-full relative"
        :style="{ height: `${rowVirtualizer.getTotalSize()}px` }"
      >
        <div
          v-for="virtualRow in rowVirtualizer.getVirtualItems()"
          :key="String(virtualRow.key)"
          class="absolute top-0 left-0 w-full flex items-center px-4 border-b border-neutral-100 dark:border-neutral-800/50 hover:bg-neutral-100 dark:hover:bg-neutral-800/80 transition-colors group cursor-pointer"
          :style="{
            height: `${virtualRow.size}px`,
            transform: `translateY(${virtualRow.start}px)`,
          }"
          @dblclick="
            handleDoubleClick(store.currentFolderNodes[virtualRow.index])
          "
        >
          <div class="flex-1 flex items-center truncate">
            <span class="mr-3 text-lg opacity-80">
              {{
                store.currentFolderNodes[virtualRow.index].is_dir ? "📁" : "📄"
              }}
            </span>
            <span class="truncate">{{
              store.currentFolderNodes[virtualRow.index].name
            }}</span>
          </div>

          <div class="w-32 text-right opacity-60">
            {{ formatSize(store.currentFolderNodes[virtualRow.index].size) }}
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
