import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { Invokes } from '../components/ui/AppProperties';
import { useEditorStore } from '../store/useEditorStore';
import { useSettingsStore } from '../store/useSettingsStore';

/// Lightroom's "Edit in Photoshop": writes the current edit as a 16-bit TIFF next to the photo, opens it in the
/// external editor, then refreshes the library so the TIFF shows up beside the original.
export async function editInExternal(path: string, refresh?: () => Promise<void>) {
  const { selectedImage, adjustments } = useEditorStore.getState();
  const editorPath = useSettingsStore.getState().appSettings?.externalEditorPath || null;
  const toastId = toast.info('Preparing a 16-bit TIFF for your editor...', { autoClose: false });
  try {
    const output = await invoke<string>(Invokes.EditInExternal, {
      path,
      adjustments: selectedImage?.path === path ? adjustments : null,
      editorPath,
    });
    toast.update(toastId, {
      render: `Opened ${output.split(/[\\/]/).pop()} in your editor`,
      type: 'success',
      autoClose: 4000,
    });
    await refresh?.();
  } catch (err) {
    toast.update(toastId, { render: `Could not open it in your editor: ${err}`, type: 'error', autoClose: 6000 });
  }
}
