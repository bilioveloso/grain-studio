import { useEffect, useState } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';

import { useLibraryStore } from '../store/useLibraryStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { useUIStore } from '../store/useUIStore';

/**
 * Drag photos from Explorer/iCloud onto the window to import them into the open library folder.
 * Opens the same import dialog as the Import button. Unlike the button, a mixed drop imports the
 * supported photos and reports what it skipped, so one stray video doesn't block the batch.
 */
export function useDropImport() {
  const { t } = useTranslation();
  const [isDragging, setIsDragging] = useState(false);

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === 'enter' || payload.type === 'over') {
        setIsDragging(true);
        return;
      }
      setIsDragging(false);
      if (payload.type !== 'drop' || payload.paths.length === 0) return;

      const targetFolder = useLibraryStore.getState().currentFolderPath;
      if (!targetFolder) {
        toast.error(t('dropImport.noFolder'));
        return;
      }

      const { supportedTypes } = useSettingsStore.getState();
      const allowed = new Set(
        [...(supportedTypes?.raw ?? []), ...(supportedTypes?.nonRaw ?? [])].map((e) => e.toLowerCase()),
      );
      const skipped = new Set<string>();
      const photos = payload.paths.filter((path) => {
        const ext = path.split('.').pop()?.toLowerCase() ?? '';
        if (allowed.has(ext)) return true;
        skipped.add(ext ? `.${ext}` : (path.split(/[\\/]/).pop() ?? path));
        return false;
      });

      if (photos.length === 0) {
        toast.error(t('dropImport.noneSupported'));
        return;
      }
      if (skipped.size > 0) toast.info(t('dropImport.skipped', { types: [...skipped].join(', ') }));

      useUIStore.getState().setUI({
        importSourcePaths: photos,
        importTargetFolder: targetFolder,
        isImportModalOpen: true,
      });
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  return { isDragging };
}
