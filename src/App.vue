<script setup lang="ts">
import { useArchiveStore } from "./stores/archive";
import Sidebar from "./components/Sidebar.vue";
import FileList from "./components/FileList.vue";

const store = useArchiveStore();

// Temporary mock trigger to load your test zip
const handleOpen = () => {
  store.loadArchive("../src-tauri/data/file.zip");
};
</script>

<template>
  <div
    class="h-screen w-screen bg-neutral-50 dark:bg-neutral-900 text-neutral-900 dark:text-neutral-100 flex flex-col overflow-hidden text-sm cursor-default select-none"
  >
    <header
      data-tauri-drag-region
      class="h-12 border-b border-neutral-200 dark:border-neutral-800 flex items-center px-4 bg-white dark:bg-neutral-950 shadow-sm z-10 shrink-0"
    >
      <button
        class="px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded-md font-medium transition-colors"
        @click="handleOpen"
      >
        Open Archive
      </button>

      <div
        class="ml-6 flex items-center space-x-2 text-neutral-500 font-medium"
      >
        <template v-for="(crumb, index) in store.breadcrumbs" :key="index">
          <span v-if="index > 0" class="text-neutral-400">/</span>
          <span
            :class="{
              'text-neutral-900 dark:text-white':
                index === store.breadcrumbs.length - 1,
            }"
            >{{ crumb }}</span
          >
        </template>
      </div>
    </header>

    <main class="flex-1 flex overflow-hidden">
      <aside
        class="w-64 border-r border-neutral-200 dark:border-neutral-800 bg-neutral-100 dark:bg-neutral-900/50 overflow-y-auto shrink-0"
      >
        <Sidebar
          v-if="store.allNodes.length > 0"
          :node="store.folderTree"
          :depth="0"
        />
      </aside>

      <section class="flex-1 overflow-hidden bg-white dark:bg-neutral-900">
        <FileList />
      </section>
    </main>
  </div>
</template>
