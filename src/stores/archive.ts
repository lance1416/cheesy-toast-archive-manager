import { defineStore } from "pinia";
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface VfsNode {
  path: string;
  name: string;
  size: number;
  is_dir: boolean;
  encoding_used: string;
}

export interface VirtualFileSystem {
  archive_path: string;
  entries: VfsNode[];
  total_entries: number;
}

export interface FolderNode {
  path: string;
  name: string;
  children: Record<string, FolderNode>;
}

export const useArchiveStore = defineStore("archive", () => {
  // State ---------------------------------------------------------------------

  const archivePath = ref<string | null>(null);
  const allNodes = ref<VfsNode[]>([]);
  const currentDirectory = ref<string>(""); // Empty string = root directory
  const isLoading = ref<boolean>(false);

  // Getters -------------------------------------------------------------------

  // Instantly derive only the items in the current folder
  const currentFolderNodes = computed(() => {
    const currentDir = currentDirectory.value; // Guaranteed to have no trailing slash

    return allNodes.value.filter((node) => {
      // Strip trailing slash from the ZIP node path for clean comparison
      const cleanNodePath = node.path.replace(/\/$/, "");

      // A directory node cannot render inside itself
      if (cleanNodePath === currentDir) return false;

      if (currentDir === "") {
        // Root Level: Include items that have no slashes in their clean path
        return !cleanNodePath.includes("/");
      } else {
        // Subfolder Level: Check if the file belongs to this exact folder
        const prefix = currentDir + "/";

        if (!node.path.startsWith(prefix)) return false;

        // Isolate the remainder of the path. If there are no more slashes,
        // it is an immediate child and not buried in a deeper sub-folder.
        const remainingPath = cleanNodePath.slice(prefix.length);
        return !remainingPath.includes("/");
      }
    });
  });

  // Breadcrumb generator for the top navigation bar
  const breadcrumbs = computed(() => {
    if (!currentDirectory.value) return ["Root"];
    return ["Root", ...currentDirectory.value.split("/")];
  });

  // Derive a lightweight nested tree ONLY for folders (for the Sidebar)
  const folderTree = computed(() => {
    const root: FolderNode = { path: "", name: "Root", children: {} };

    const directories = allNodes.value.filter((n) => n.is_dir);

    directories.forEach((dir) => {
      // Strip trailing slash before splitting so we don't get empty array parts
      const cleanPath = dir.path.replace(/\/$/, "");
      if (!cleanPath) return; // Skip if it somehow was just '/'

      const parts = cleanPath.split("/");
      let currentLevel = root.children;

      parts.forEach((part, index) => {
        if (!currentLevel[part]) {
          const currentPath = parts.slice(0, index + 1).join("/");
          currentLevel[part] = { path: currentPath, name: part, children: {} };
        }
        currentLevel = currentLevel[part].children;
      });
    });

    return root;
  });

  // Actions -------------------------------------------------------------------
  async function loadArchive(filePath: string) {
    isLoading.value = true;
    try {
      // Cross the IPC boundary to hit our Rust `open_archive` command
      const vfs = await invoke<VirtualFileSystem>("open_archive", {
        path: filePath,
        fallbackEncoding: null,
      });

      archivePath.value = vfs.archive_path;
      allNodes.value = vfs.entries;
      currentDirectory.value = ""; // Reset to root
    } catch (error) {
      console.error("Failed to load archive:", error);
      // Here you would trigger a Vue toast notification showing the CheesyError string
    } finally {
      isLoading.value = false;
    }
  }

  function navigateTo(folderPath: string) {
    currentDirectory.value = folderPath.replace(/\/$/, "");
  }

  function navigateUp() {
    if (!currentDirectory.value) return;
    const parts = currentDirectory.value.split("/");
    parts.pop(); // Remove the last folder
    currentDirectory.value = parts.join("/");
  }

  return {
    // State
    archivePath,
    allNodes,
    currentDirectory,
    isLoading,
    // Getters
    currentFolderNodes,
    breadcrumbs,
    folderTree,
    // Actions
    loadArchive,
    navigateTo,
    navigateUp,
  };
});
