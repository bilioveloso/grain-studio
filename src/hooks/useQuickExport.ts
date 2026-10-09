import { useCallback, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { join, pictureDir } from '@tauri-apps/api/path';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';

import { Invokes } from '../components/ui/AppProperties';
import { ExportPreset, ExportSettings, FILE_FORMATS, Status } from '../components/ui/ExportImportProperties';
import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { useProcessStore } from '../store/useProcessStore';
import { useSettingsStore } from '../store/useSettingsStore';

// Internal bookkeeping entry, not a user preset.
const LAST_USED_PRESET_ID = '__last_used__';

function presetToSettings(preset: ExportPreset, cleanMode: boolean): ExportSettings {
  return {
    filenameTemplate: preset.filenameTemplate,
    jpegQuality: preset.jpegQuality,
    tiffBitDepth: preset.tiffBitDepth ?? 16,
    keepMetadata: preset.keepMetadata,
    preserveTimestamps: preset.preserveTimestamps ?? false,
    preserveFolders: false,
    destinationType: 'customFolder',
    subfolder: '',
    resize: preset.enableResize
      ? { mode: preset.resizeMode, value: preset.resizeValue, dontEnlarge: preset.dontEnlarge }
      : null,
    outputSharpening: preset.outputSharpening ?? 0,
    // Clean Mode never sends AI upscaling, even if a preset asks for it.
    upscaleMethod: cleanMode ? 'classic' : (preset.upscaleMethod ?? 'classic'),
    stripGps: preset.stripGps,
    exportMasks: false,
    watermark:
      preset.enableWatermark && preset.watermarkPath
        ? {
            path: preset.watermarkPath,
            anchor: preset.watermarkAnchor as any,
            scale: preset.watermarkScale,
            spacing: preset.watermarkSpacing,
            opacity: preset.watermarkOpacity,
          }
        : null,
  };
}

/**
 * One-click export of the open photo with a saved preset, straight into Pictures\Studio\03_finished.
 * No dialogs: the Studio folder is the destination so the edit → export flow never stops.
 */
export function useQuickExport() {
  const { t } = useTranslation();
  const presets = useSettingsStore((s) => s.appSettings?.exportPresets ?? []).filter(
    (p: ExportPreset) => p.id !== LAST_USED_PRESET_ID,
  );
  const status = useProcessStore((s) => s.exportState.status);
  const errorMessage = useProcessStore((s) => s.exportState.errorMessage);
  const pendingRef = useRef<string | null>(null);

  // Toast only for exports started here, so the Export panel's own exports stay quiet.
  useEffect(() => {
    if (!pendingRef.current) return;
    if (status === Status.Success) {
      toast.success(t('quickExport.done', { preset: pendingRef.current }));
      pendingRef.current = null;
    } else if (status === Status.Error) {
      toast.error(t('quickExport.failed', { error: errorMessage }));
      pendingRef.current = null;
    }
  }, [status, errorMessage, t]);

  const exportWith = useCallback(
    async (preset: ExportPreset) => {
      const { selectedImage, adjustments } = useEditorStore.getState();
      if (!selectedImage) return;
      const cleanMode = useSettingsStore.getState().appSettings?.cleanMode ?? true;
      const format = FILE_FORMATS.find((f) => f.id === preset.fileFormat) ?? FILE_FORMATS[0];

      try {
        const outputFolder = await join(await pictureDir(), 'Studio', '03_finished');
        pendingRef.current = preset.name;
        useProcessStore
          .getState()
          .setExportState({ status: Status.Exporting, progress: { current: 0, total: 1 }, errorMessage: '' });
        await invoke(Invokes.ExportImages, {
          paths: [selectedImage.path],
          outputFolderOrFile: outputFolder,
          isExplicitFilePath: false,
          baseOriginFolders: useLibraryStore.getState().rootPaths,
          exportSettings: presetToSettings(preset, cleanMode),
          outputFormat: format.extensions[0],
          currentEditPath: selectedImage.path,
          currentEditAdjustments: adjustments,
        });
      } catch (error: any) {
        pendingRef.current = null;
        useProcessStore.getState().setExportState({ status: Status.Error, errorMessage: String(error) });
        toast.error(t('quickExport.failed', { error: String(error) }));
      }
    },
    [t],
  );

  return { presets, exportWith, isExporting: status === Status.Exporting || status === Status.Cancelling };
}
