import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { Invokes, ImageFile } from '../components/ui/AppProperties';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { expandGroupedPaths } from './imageGrouping';

/// Lightroom-style flags, kept apart from colour labels. Stored as tags like colour labels are.
export type Flag = 'pick' | 'reject';
export const FLAG_PREFIX = 'flag:';

export function getFlag(tags?: string[] | null): Flag | null {
  const tag = tags?.find((t) => t.startsWith(FLAG_PREFIX));
  return tag ? (tag.slice(FLAG_PREFIX.length) as Flag) : null;
}

/// P / X / U: pick, reject or clear the flag of the selected photos (or of `paths`).
export async function setFlag(flag: Flag | null, paths?: string[]) {
  const { multiSelectedPaths, imageList, setLibrary } = useLibraryStore.getState();
  const { selectedImage } = useEditorStore.getState();
  const selected =
    paths || (multiSelectedPaths.length > 0 ? multiSelectedPaths : selectedImage ? [selectedImage.path] : []);
  if (selected.length === 0) return;

  const grouping = useSettingsStore.getState().appSettings?.grouping ?? 'off';
  const targets = expandGroupedPaths(imageList, selected, grouping);

  try {
    for (const other of ['pick', 'reject'] as const) {
      if (other !== flag) await invoke(Invokes.RemoveTagForPaths, { paths: targets, tag: FLAG_PREFIX + other });
    }
    if (flag) await invoke(Invokes.AddTagForPaths, { paths: targets, tag: FLAG_PREFIX + flag });

    setLibrary((state) => ({
      imageList: state.imageList.map((image: ImageFile) => {
        if (!targets.includes(image.path)) return image;
        const others = (image.tags || []).filter((t: string) => !t.startsWith(FLAG_PREFIX));
        return { ...image, tags: flag ? [...others, FLAG_PREFIX + flag] : others };
      }),
    }));
  } catch (err) {
    toast.error(`Could not set the flag: ${err}`);
  }
}
