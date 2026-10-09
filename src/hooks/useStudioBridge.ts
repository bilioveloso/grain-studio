import { useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import { useEditorStore } from '../store/useEditorStore';
import { useLibraryStore } from '../store/useLibraryStore';
import { Invokes } from '../components/ui/AppProperties';
import { Adjustments } from '../utils/adjustments';
import { debouncedSetHistory } from './useEditorActions';
import { useAiMasking } from './useAiMasking';
import { v4 as uuidv4 } from 'uuid';
import { INITIAL_MASK_CONTAINER, MaskContainer } from '../utils/adjustments';
import { createSubMask } from '../utils/maskUtils';
import { getOrientedDimensions } from '../utils/cropUtils';
import { Mask, SubMask, SubMaskMode } from '../components/panel/right/Masks';

/**
 * Studio bridge: executes requests from the local Claude MCP server against the live editor.
 * The backend emits `studio-bridge-request` {id, method, params}; we answer with
 * `studio_bridge_respond`. Edits go through the same store path as slider changes,
 * so they get undo history and the normal auto-save.
 */

interface BridgeRequest {
  id: string;
  method: string;
  params?: any;
}

const OMITTED = '<omitted>';
const OPEN_TIMEOUT_MS = 20000;

const isPlainObject = (v: unknown): v is Record<string, any> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

// Objects merge key by key; arrays (masks, curves) and scalars replace.
function deepMerge(base: any, patch: any): any {
  if (!isPlainObject(base) || !isPlainObject(patch)) return patch;
  const out: Record<string, any> = { ...base };
  for (const [key, value] of Object.entries(patch)) {
    out[key] = isPlainObject(value) && isPlainObject(base[key]) ? deepMerge(base[key], value) : value;
  }
  return out;
}

// Large embedded bitmaps are useless to Claude and would blow up the response.
function summarizeAdjustments(adjustments: Adjustments): any {
  const copy: any = structuredClone(adjustments);
  if (copy.lensBlurDepthMap) copy.lensBlurDepthMap = OMITTED;
  if (copy.lutData) copy.lutData = OMITTED;
  for (const mask of copy.masks ?? []) {
    for (const sub of mask.subMasks ?? []) {
      if (sub.parameters?.maskDataBase64) sub.parameters.maskDataBase64 = OMITTED;
    }
  }
  for (const patch of copy.aiPatches ?? []) {
    for (const key of Object.keys(patch)) {
      if (typeof patch[key] === 'string' && patch[key].length > 512) patch[key] = OMITTED;
    }
  }
  return copy;
}

function editorSummary() {
  const { selectedImage, adjustments, historyIndex } = useEditorStore.getState();
  return {
    image: selectedImage
      ? {
          path: selectedImage.path,
          width: selectedImage.width,
          height: selectedImage.height,
          isRaw: selectedImage.isRaw,
          isReady: selectedImage.isReady,
        }
      : null,
    historyIndex,
    adjustments: selectedImage ? summarizeAdjustments(adjustments) : null,
  };
}

function applyAdjustments(next: Adjustments) {
  useEditorStore.getState().setEditor((state) => ({
    adjustments: next,
    ...(state.showOriginal ? { showOriginal: false, previewOverride: null } : {}),
  }));
  debouncedSetHistory(next);
}

// Map [x, y] in 0..1 of the cropped, oriented photo (capture_mask frame) to image pixels.
export function previewToImage(selectedImage: any, adjustments: Adjustments) {
  const { width: W, height: H } = getOrientedDimensions(
    selectedImage.width,
    selectedImage.height,
    adjustments.orientationSteps || 0,
  );
  const crop: any = adjustments.crop;
  const pct = crop?.unit === '%';
  const cx = crop ? (pct ? (crop.x / 100) * W : crop.x) : 0;
  const cy = crop ? (pct ? (crop.y / 100) * H : crop.y) : 0;
  const cw = crop ? (pct ? (crop.width / 100) * W : crop.width) : W;
  const ch = crop ? (pct ? (crop.height / 100) * H : crop.height) : H;
  const unit = (v: unknown) => Math.min(1, Math.max(0, Number(v) || 0));
  return (p: number[]) => ({ x: cx + unit(p?.[0]) * cw, y: cy + unit(p?.[1]) * ch });
}

async function waitForImage(path: string) {
  const start = Date.now();
  while (Date.now() - start < OPEN_TIMEOUT_MS) {
    const { selectedImage } = useEditorStore.getState();
    if (selectedImage?.path === path && selectedImage.isReady) return;
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`Timed out opening ${path}`);
}

export function useStudioBridge(handleImageSelect: (path: string) => void | Promise<void>) {
  const handleImageSelectRef = useRef(handleImageSelect);
  const { handleGenerateAiMask, handleGenerateAiSkyMask } = useAiMasking();
  const generateMaskRef = useRef(handleGenerateAiMask);
  const generateSkyRef = useRef(handleGenerateAiSkyMask);
  useEffect(() => {
    handleImageSelectRef.current = handleImageSelect;
    generateMaskRef.current = handleGenerateAiMask;
    generateSkyRef.current = handleGenerateAiSkyMask;
  });

  // Other parts of the app open a file by path this way (the versions menu opens Apple's render of an iPhone edit).
  useEffect(() => {
    const onOpen = (e: Event) => handleImageSelectRef.current((e as CustomEvent<string>).detail);
    window.addEventListener('studio-open-image', onOpen);
    return () => window.removeEventListener('studio-open-image', onOpen);
  }, []);

  useEffect(() => {
    const handlers: Record<string, (params: any) => Promise<any> | any> = {
      get_state: () => editorSummary(),

      // Full, unstripped edit for backend rendering (mask bitmaps included); used by capture_preview.
      // The reference photo the user picked for Claude to match.
      get_reference: () => {
        const { referenceImagePath } = useEditorStore.getState();
        if (!referenceImagePath)
          throw new Error('No reference photo picked. Ask the user to pick one with the Match a look button.');
        return { path: referenceImagePath };
      },

      save_version: async ({ name }: { name?: string }) => {
        const { selectedImage, adjustments } = useEditorStore.getState();
        if (!selectedImage?.path) throw new Error('No image open.');
        await invoke(Invokes.SaveVersion, {
          path: selectedImage.path,
          name: String(name || '').trim(),
          adjustments,
        });
        window.dispatchEvent(new CustomEvent('studio-versions-changed'));
        return { saved: name || '' };
      },

      get_render_input: () => {
        const { selectedImage, adjustments } = useEditorStore.getState();
        if (!selectedImage?.isReady) throw new Error('No image open.');
        // The frame a mask is measured in: the cropped, oriented photo, in its own pixels.
        const topLeft = previewToImage(selectedImage, adjustments)([0, 0]);
        const bottomRight = previewToImage(selectedImage, adjustments)([1, 1]);
        return {
          path: selectedImage.path,
          adjustments,
          frame: {
            offsetX: topLeft.x,
            offsetY: topLeft.y,
            width: bottomRight.x - topLeft.x,
            height: bottomRight.y - topLeft.y,
          },
        };
      },

      list_images: () =>
        useLibraryStore.getState().imageList.map((img) => ({
          path: img.path,
          rating: img.rating,
          isEdited: img.is_edited,
          isRaw: img.is_raw,
        })),

      open_image: async ({ path }: { path: string }) => {
        if (!path) throw new Error('path is required');
        await handleImageSelectRef.current(path);
        await waitForImage(path);
        return editorSummary();
      },

      set_adjustments: async ({ patch }: { patch: Record<string, any> }) => {
        const { selectedImage, adjustments } = useEditorStore.getState();
        if (!selectedImage) throw new Error('No image open.');
        if (!isPlainObject(patch)) throw new Error('patch must be an object of adjustment fields');
        // get_state shows the depth map as a placeholder; echoed back, it must not replace the real one.
        if (patch.lensBlurDepthMap === OMITTED) delete patch.lensBlurDepthMap;
        const next = deepMerge(adjustments, patch);
        applyAdjustments(next);
        // Lens blur reads a depth map. The Effects panel makes one when its switch is turned on; a lens blur
        // switched on from here had none, and silently changed nothing. Make it the same way, then attach it.
        if (next.lensBlurEnabled && !next.lensBlurDepthMap) {
          const depth: string = await invoke('generate_full_image_depth_map', { jsAdjustments: next });
          applyAdjustments({ ...useEditorStore.getState().adjustments, lensBlurDepthMap: depth });
        }
        return editorSummary();
      },

      // Smart mask: Claude picks points on the preview (0..1 of the cropped frame); SAM draws the mask.
      // Lightroom-style People/Landscape masks from Claude: one SAM sub-mask per region
      // (include/exclude points, optional box), added to a new or an existing mask.
      create_smart_mask: async ({
        name,
        regions,
        maskId,
        invert,
      }: {
        name?: string;
        regions: any[];
        maskId?: string;
        invert?: boolean;
      }) => {
        const { selectedImage, adjustments } = useEditorStore.getState();
        if (!selectedImage?.isReady) throw new Error('No image open.');
        if (!Array.isArray(regions) || regions.length === 0 || regions.length > 8)
          throw new Error('regions must be 1-8 objects: {points, exclude_points?, box?, subtract?}');
        const toImage = previewToImage(selectedImage, adjustments);
        // "Sky" has its own model, and it is steadier than asking the point-based one to find sky from a few
        // clicks -- it knows what sky is, including between branches and through a railing. One whole region
        // called sky means that model; anything else, or sky as one part of a bigger mask, goes the usual way.
        const wantsSky =
          !maskId &&
          regions.length === 1 &&
          !regions[0]?.subtract &&
          (name || '').trim().toLowerCase() === 'sky';
        const subMasks: any[] = regions.map((r) => {
          if (!wantsSky && !r?.points?.length && !(Array.isArray(r?.box) && r.box.length === 4))
            throw new Error('Each region needs include points or a box.');
          return createSubMask(
            wantsSky ? Mask.AiSky : Mask.AiSubject,
            selectedImage as any,
            r.subtract ? SubMaskMode.Subtractive : SubMaskMode.Additive,
          );
        });

        let container: MaskContainer;
        if (maskId) {
          const existing = adjustments.masks?.find((m: MaskContainer) => m.id === maskId);
          if (!existing) throw new Error(`No mask with id ${maskId}`);
          container = { ...existing, subMasks: [...existing.subMasks, ...subMasks] };
          applyAdjustments({
            ...adjustments,
            masks: adjustments.masks.map((m: MaskContainer) => (m.id === maskId ? container : m)),
          });
        } else {
          container = {
            ...INITIAL_MASK_CONTAINER,
            id: uuidv4(),
            name: name || 'Smart mask',
            invert: !!invert,
            subMasks,
          };
          applyAdjustments({ ...adjustments, masks: [...(adjustments.masks || []), container] });
        }

        for (const [i, r] of regions.entries()) {
          const include = (r.points || []).map((pt: number[]) => ({ ...toImage(pt), label: 1 }));
          const exclude = (r.exclude_points || []).map((pt: number[]) => ({ ...toImage(pt), label: 0 }));
          const boxed = Array.isArray(r.box) && r.box.length === 4;
          const start = boxed ? toImage([r.box[0], r.box[1]]) : include[0];
          const end = boxed ? toImage([r.box[2], r.box[3]]) : include[0];
          if (wantsSky) {
            await generateSkyRef.current(subMasks[i].id);
          } else {
            await generateMaskRef.current(subMasks[i].id, start, end, [...include, ...exclude]);
          }
          const made: any = useEditorStore
            .getState()
            .adjustments.masks?.flatMap((m: MaskContainer) => m.subMasks)
            .find((sm: any) => sm.id === subMasks[i].id);
          if (!made?.parameters?.maskDataBase64)
            throw new Error(`Region ${i + 1} produced no mask (the app shows the error).`);
        }
        return {
          maskId: container.id,
          name: container.name,
          regionsAdded: subMasks.length,
          next: 'Check it with capture_mask({mask_id}) before editing inside it.',
        };
      },

      // Limit a mask to a sampled color or brightness (Intersect sub-mask).
      refine_mask: ({
        maskId,
        by,
        point,
        tolerance,
      }: {
        maskId: string;
        by: string;
        point: number[];
        tolerance?: number;
      }) => {
        const { selectedImage, adjustments } = useEditorStore.getState();
        if (!selectedImage?.isReady) throw new Error('No image open.');
        if (by !== 'color' && by !== 'luminance') throw new Error('by must be "color" or "luminance"');
        if (!adjustments.masks?.some((m: MaskContainer) => m.id === maskId))
          throw new Error(`No mask with id ${maskId}`);
        const { x, y } = previewToImage(selectedImage, adjustments)(point);
        const sub: any = createSubMask(
          by === 'color' ? Mask.Color : Mask.Luminance,
          selectedImage as any,
          SubMaskMode.Intersect,
        );
        sub.parameters = {
          targetX: x,
          targetY: y,
          tolerance: Math.min(100, Math.max(1, Number(tolerance ?? 30))),
          rotation: adjustments.rotation || 0,
          flipHorizontal: !!adjustments.flipHorizontal,
          flipVertical: !!adjustments.flipVertical,
          orientationSteps: adjustments.orientationSteps || 0,
        };
        applyAdjustments({
          ...adjustments,
          masks: adjustments.masks.map((m: MaskContainer) =>
            m.id === maskId ? { ...m, subMasks: [...m.subMasks, sub] } : m,
          ),
        });
        return {
          maskId,
          limitedBy: by,
          next: 'Check it with capture_mask({mask_id}). Too tight: undo and raise tolerance.',
        };
      },

      // Edit one mask: its adjustments (deep-merged) and name/invert/opacity/visible.
      update_mask: ({ maskId, patch }: { maskId: string; patch: Record<string, any> }) => {
        const { adjustments } = useEditorStore.getState();
        const target = adjustments.masks?.find((m: MaskContainer) => m.id === maskId);
        if (!target) throw new Error(`No mask with id ${maskId}`);
        if (!isPlainObject(patch)) throw new Error('patch must be an object');
        const { adjustments: adjPatch, edges, ...rest } = patch;
        if (edges !== undefined && !isPlainObject(edges)) throw new Error('edges must be an object');
        const edgeKeys = ['refine', 'contrast', 'grow', 'feather'];
        const edgePatch = Object.fromEntries(
          Object.entries(edges ?? {}).filter(([k, v]) => edgeKeys.includes(k) && typeof v === 'number'),
        );
        const masks = adjustments.masks.map((m: MaskContainer) =>
          m.id === maskId
            ? {
                ...m,
                ...rest,
                adjustments: adjPatch ? deepMerge(m.adjustments, adjPatch) : m.adjustments,
                subMasks: m.subMasks.map((sm: SubMask) =>
                  [Mask.AiSubject, Mask.AiForeground, Mask.AiSky].includes(sm.type)
                    ? { ...sm, parameters: { ...sm.parameters, ...edgePatch } }
                    : sm,
                ),
              }
            : m,
        );
        applyAdjustments({ ...adjustments, masks });
        return editorSummary();
      },

      undo: () => {
        useEditorStore.getState().undo();
        return editorSummary();
      },

      redo: () => {
        useEditorStore.getState().redo();
        return editorSummary();
      },
    };

    const unlisten = listen<BridgeRequest>('studio-bridge-request', async ({ payload }) => {
      const { id, method, params } = payload;
      try {
        const handler = handlers[method];
        if (!handler) throw new Error(`Unknown bridge method: ${method}`);
        const result = await handler(params ?? {});
        await invoke(Invokes.StudioBridgeRespond, { id, ok: true, result });
      } catch (err: any) {
        await invoke(Invokes.StudioBridgeRespond, { id, ok: false, result: String(err?.message ?? err) });
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);
}
