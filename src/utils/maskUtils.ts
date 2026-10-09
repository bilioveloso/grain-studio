import { v4 as uuidv4 } from 'uuid';
import { Mask, SubMaskMode, formatMaskTypeName } from '../components/panel/right/Masks';
import { ImageDimensions } from '../hooks/useImageRenderSize';

export const createSubMask = (
  type: Mask,
  imageDimensions: ImageDimensions,
  mode: SubMaskMode = SubMaskMode.Additive
) => {
  const { width, height } = imageDimensions || { width: 1000, height: 1000 };
  const common = {
    id: uuidv4(),
    visible: true,
    invert: false,
    opacity: 100,
    mode,
    name: formatMaskTypeName(type),
    type,
  };

  switch (type) {
    case Mask.Radial:
      return {
        ...common,
        parameters: {
          centerX: width / 2,
          centerY: height / 2,
          radiusX: width / 4,
          radiusY: width / 4,
          rotation: 0,
          feather: 0.5,
        },
      };
    case Mask.Linear:
      return {
        ...common,
        parameters: { startX: width * 0.25, startY: height / 2, endX: width * 0.75, endY: height / 2, range: 50 },
      };
    case Mask.Brush:
      return { ...common, parameters: { lines: [] } };
    case Mask.Flow:
      return { ...common, parameters: { lines: [], flow: 10 } };
    // New AI masks start refined onto the photo's edges; older ones (no refine key) stay as they were made.
    // The models run small -- 320 px for sky and foreground, 1024 for the subject finder -- so what they return
    // is a wide soft ramp once it is scaled up to a real photo: on a 3024 x 4032 frame, measured, 2.5% of the
    // whole picture was half-masked haze, which reads as a glow around the subject. A little contrast collapses
    // that: 15 takes it to 0.6% and the subject's own coverage does not move at all. Past about 25 it stops
    // helping and only hardens, so the default sits just the useful side of the knee.
    // Subject masks are the exception now: they come back from crisp_mask.rs already matted onto the photo's
    // edge, with real partial values through lattices and feather wisps. Refine would re-blur that on a 1536 px
    // working copy, and contrast would crush the partial values -- both undo exactly what the matte is for.
    case Mask.AiSubject:
      return { ...common, parameters: { maskDataBase64: null, grow: 0, feather: 0, refine: 0, contrast: 0 } };
    case Mask.AiForeground:
    case Mask.AiSky:
      return { ...common, parameters: { maskDataBase64: null, grow: 0, feather: 0, refine: 50, contrast: 20 } };
    case Mask.Polygon:
      return { ...common, parameters: { points: [] as Array<{ x: number; y: number }>, grow: 0, feather: 8, refine: 0, contrast: 0 } };
    case Mask.QuickEraser:
      return { ...common, parameters: { maskDataBase64: null, grow: 75, feather: 50 } };
    default:
      return { ...common, parameters: {} };
  }
};

/**
 * Whether to throw away the model's cut of a circled area and keep the shape that was drawn.
 *
 * The model is being asked "what is the thing inside this circle", and there are two ways for its answer to be
 * worthless. It can hand back the whole box, having found no edges at all, in which case intersecting is just
 * the circle again with a softer and haze-prone edge. Or it can find a detail -- a glint on the water, one leaf
 * -- and intersecting the circle with that leaves a mask that edits almost nothing, which reads as the app
 * ignoring you.
 *
 * Area alone cannot tell the second case from a genuinely thin subject: a mast, a wire, a railing all cover
 * very little of a loosely drawn circle, and those are exactly the cases where a real mask beats a drawn one.
 * What separates them is reach. A thin subject still runs most of the way across the circle someone drew around
 * it; a speck sits in one corner. So the test is how far the cut spans, not how much it fills. Measured on a
 * real miss: a glint in open water spanned 0.35 of the circle, while a crane or a bird spans nearly all of it.
 */
export const DRAWN_SHAPE_CEILING = 0.96;
export const DRAWN_SHAPE_MIN_REACH = 0.5;

export interface CutShape {
  /** How much of the drawn ring's box the cut fills, 0..1. */
  coverage: number;
  /** How far the cut reaches across that box in its longer direction, 0..1. */
  reach: number;
}

export const shouldKeepDrawnShape = ({ coverage, reach }: CutShape): boolean =>
  !Number.isFinite(coverage) ||
  !Number.isFinite(reach) ||
  coverage >= DRAWN_SHAPE_CEILING ||
  reach < DRAWN_SHAPE_MIN_REACH;

/**
 * How much of `box` (in image pixels) the mask bitmap fills and how far it reaches across, measured on a small
 * copy because a decision this coarse does not need eight thousand pixels to make.
 */
export async function measureCut(
  dataUrl: string,
  box: { x: number; y: number; w: number; h: number },
): Promise<CutShape> {
  const nothing = { coverage: NaN, reach: NaN };
  if (!dataUrl || box.w <= 0 || box.h <= 0) return nothing;
  const bitmap = await new Promise<HTMLImageElement | null>((resolve) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => resolve(null);
    img.src = dataUrl;
  });
  if (!bitmap) return nothing;
  const side = 96;
  const canvas = document.createElement('canvas');
  canvas.width = side;
  canvas.height = side;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  if (!ctx) return nothing;
  ctx.drawImage(bitmap, box.x, box.y, box.w, box.h, 0, 0, side, side);
  const { data } = ctx.getImageData(0, 0, side, side);
  let on = 0;
  let minX = side;
  let maxX = -1;
  let minY = side;
  let maxY = -1;
  for (let i = 0; i < data.length; i += 4) {
    // The bitmap is greyscale on an opaque canvas; anything past halfway counts as masked.
    if (data[i] <= 127) continue;
    const px = (i / 4) % side;
    const py = Math.floor(i / 4 / side);
    on += 1;
    if (px < minX) minX = px;
    if (px > maxX) maxX = px;
    if (py < minY) minY = py;
    if (py > maxY) maxY = py;
  }
  if (maxX < 0) return { coverage: 0, reach: 0 };
  return {
    coverage: on / (side * side),
    reach: Math.max((maxX - minX + 1) / side, (maxY - minY + 1) / side),
  };
}
