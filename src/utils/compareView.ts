import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { Invokes } from '../components/ui/AppProperties';
import { useEditorStore } from '../store/useEditorStore';
import { Adjustments, INITIAL_ADJUSTMENTS } from './adjustments';

/// Kept from the edit when showing "before": the unedited photo still gets the same crop, rotation and lens
/// geometry, so it lines up with the edited one.
export const GEOMETRY_KEYS: Array<keyof Adjustments> = [
  'crop',
  'rotation',
  'flipHorizontal',
  'flipVertical',
  'orientationSteps',
  'aspectRatio',
  'transformDistortion',
  'transformVertical',
  'transformHorizontal',
  'transformRotate',
  'transformAspect',
  'transformScale',
  'transformXOffset',
  'transformYOffset',
  'lensDistortionAmount',
  'lensVignetteAmount',
  'lensTcaAmount',
  'lensDistortionParams',
  'lensMaker',
  'lensModel',
  'lensDistortionEnabled',
  'lensTcaEnabled',
  'lensVignetteEnabled',
];

export function originalAdjustments(adjustments: Adjustments): Adjustments {
  const original = { ...INITIAL_ADJUSTMENTS } as Record<string, unknown>;
  GEOMETRY_KEYS.forEach((key) => {
    original[key] = adjustments[key];
  });
  return original as unknown as Adjustments;
}

export function clearCompareView() {
  const { beforeUrl, compareMode, setEditor } = useEditorStore.getState();
  if (beforeUrl) URL.revokeObjectURL(beforeUrl);
  if (compareMode || beforeUrl) setEditor({ compareMode: false, beforeUrl: null });
}

/// Lightroom's Y: before and after side by side. The before is rendered once; the after stays live.
export async function toggleCompareView() {
  const { compareMode, adjustments, setEditor } = useEditorStore.getState();
  if (compareMode) {
    clearCompareView();
    return;
  }
  try {
    const bytes: ArrayLike<number> = await invoke(Invokes.GeneratePresetPreview, {
      jsAdjustments: originalAdjustments(adjustments),
      maxDim: 2560,
    });
    const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' }));
    setEditor({ compareMode: true, beforeUrl: url });
  } catch (err) {
    toast.error(`Could not render the original: ${err}`);
  }
}
