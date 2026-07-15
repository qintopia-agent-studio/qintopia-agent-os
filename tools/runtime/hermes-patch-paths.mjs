export const extractPatchEntries = (patchText) =>
  [...patchText.matchAll(/^diff --git a\/(.+?) b\/(.+)$/gm)].map((match) => ({
    sourcePath: match[1],
    targetPath: match[2],
  }));

export const patchEntriesMatchAllowedPaths = (entries, allowedPaths) =>
  entries.length === allowedPaths.length &&
  entries.every(
    (entry, index) =>
      entry.sourcePath === allowedPaths[index] &&
      entry.targetPath === allowedPaths[index]
  );
