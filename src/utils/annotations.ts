/**
 * Places the user marked on the photo, to talk to the assistant about.
 *
 * ChatGPT sends its pins to an image model as a hint on a picture, and says so plainly: "a location is not a
 * pixel lock". Here a mark is a coordinate in the same 0..1 frame the studio tools use, so the assistant can
 * build a real mask there and change only what is inside it; everything else keeps its own pixels.
 */
export interface Point {
  x: number;
  y: number;
}

export interface Annotation {
  id: string;
  kind: 'pin' | 'ring';
  /** Where the note sits: the pin itself, or the top of the ring. 0..1 of the frame capture_mask shows. */
  x: number;
  y: number;
  /** The drawn ring, 0..1, closed implicitly. Empty for a pin. */
  points: Point[];
  note: string;
}

export const clamp01 = (v: number) => Math.min(1, Math.max(0, v));
const id = () => `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;

export const newPin = (x: number, y: number): Annotation => ({
  id: id(),
  kind: 'pin',
  x: clamp01(x),
  y: clamp01(y),
  points: [],
  note: '',
});

export const newRing = (points: Point[]): Annotation => {
  const cleaned = points.map((p) => ({ x: clamp01(p.x), y: clamp01(p.y) }));
  const top = cleaned.reduce((a, b) => (b.y < a.y ? b : a), cleaned[0]);
  return { id: id(), kind: 'ring', x: top.x, y: top.y, points: cleaned, note: '' };
};

/** [x1, y1, x2, y2] around the drawn ring. */
export function ringBox(points: Point[]): [number, number, number, number] {
  const xs = points.map((p) => p.x);
  const ys = points.map((p) => p.y);
  return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
}

/**
 * The box the mask model is seeded with: the ring, loosened a little. The model can only return what is inside
 * the box it was given, so a box drawn exactly on the ring makes it cut the subject off in a straight line at
 * the ring's edge. Letting it see slightly past the ring lets it end on the subject's own edge instead; the
 * drawn ring still trims the result, because it is intersected over the top.
 */
export function padBox(box: [number, number, number, number], by = 0.06): [number, number, number, number] {
  const [x1, y1, x2, y2] = box;
  const px = (x2 - x1) * by;
  const py = (y2 - y1) * by;
  return [clamp01(x1 - px), clamp01(y1 - py), clamp01(x2 + px), clamp01(y2 + py)];
}

/** Even-odd test, so a ring drawn in either direction counts the same. */
export function inRing(points: Point[], p: Point): boolean {
  let inside = false;
  for (let i = 0, j = points.length - 1; i < points.length; j = i++) {
    const a = points[i];
    const b = points[j];
    if (a.y > p.y !== b.y > p.y && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y || 1e-9) + a.x) inside = !inside;
  }
  return inside;
}

/** The middle of the drawn ring, which is where the thing you circled almost always is. */
function centroid(points: Point[]): Point {
  const n = points.length;
  return {
    x: points.reduce((a, p) => a + p.x, 0) / n,
    y: points.reduce((a, p) => a + p.y, 0) / n,
  };
}

/**
 * A handful of points the mask model can be seeded with, taken along the ring's longer axis through its middle.
 * Spreading them over the ring's whole area sounds fairer and is worse: a loose circle round a stork is mostly
 * sky, so an even spread hands the model five sky points and it selects the sky, exactly as asked. Clustering
 * them all in the middle is no better -- the model then finds only the part of the bird under the cluster. What
 * people circle tends to fill the long way of the circle they drew, so a line down that axis walks the subject
 * from end to end: the stork's head, neck, body and legs, the crane's jib, tower and base. Falls back to the box
 * centre when a ring is too thin to hold any point.
 */
export function interiorPoints(points: Point[], limit = 7): Point[] {
  if (points.length < 3) return [];
  const [x1, y1, x2, y2] = ringBox(points);
  // A ring barely bigger than a dot means one place, not five points a thousandth apart.
  if (x2 - x1 < 0.01 && y2 - y1 < 0.01) return [{ x: (x1 + x2) / 2, y: (y1 + y2) / 2 }];
  const mid = centroid(points);
  const tall = y2 - y1 >= x2 - x1;
  const half = (tall ? y2 - y1 : x2 - x1) / 2;
  const found: Point[] = [];
  // Out from the middle in both directions, so the chosen few always straddle the centre.
  for (let step = 0; step <= 8; step++) {
    for (const dir of step === 0 ? [0] : [-1, 1]) {
      const off = (dir * step * half) / 8;
      const p = tall ? { x: mid.x, y: mid.y + off } : { x: mid.x + off, y: mid.y };
      if (inRing(points, p)) found.push(p);
    }
  }
  if (!found.length) return [{ x: (x1 + x2) / 2, y: (y1 + y2) / 2 }];
  // A line of points reads as a line. Two more across it, close in where the subject is thickest, say "a shape".
  const across = (tall ? x2 - x1 : y2 - y1) / 2;
  const wings = [-0.2, 0.2]
    .map((f) => (tall ? { x: mid.x + f * across, y: mid.y } : { x: mid.x, y: mid.y + f * across }))
    .filter((p) => inRing(points, p));
  if (found.length + wings.length <= limit) return [...found, ...wings];
  // Thin the line out evenly rather than keeping only its middle, and keep the width.
  const keep = Math.max(2, limit - wings.length);
  const stride = (found.length - 1) / (keep - 1);
  return [...Array.from({ length: keep }, (_, i) => found[Math.round(i * stride)]), ...wings];
}

const place = (p: Point) => `(${p.x.toFixed(3)}, ${p.y.toFixed(3)})`;

function describe(a: Annotation, index: number, fallbackNote: string, maskId?: string): string {
  const note = a.note.trim() || fallbackNote;
  if (a.kind === 'ring' && maskId) {
    return `${index}. the area I circled, already a mask (id ${maskId}) — ${note}`;
  }
  if (a.kind === 'ring' && a.points.length >= 3) {
    const [x1, y1, x2, y2] = ringBox(a.points);
    const seeds = interiorPoints(a.points).map(place).join(', ');
    return `${index}. I circled an area — box [${x1.toFixed(3)}, ${y1.toFixed(3)}, ${x2.toFixed(3)}, ${y2.toFixed(
      3,
    )}], points inside it: ${seeds} — ${note}`;
  }
  return `${index}. ${place(a)} — ${note}`;
}

/**
 * The message the assistant receives. Shaped as target / change only / keep unchanged, which is the wording that
 * works for pinned edits, with the last part guaranteed by the pipeline rather than merely asked for. A circled
 * area carries its box and seed points, so the mask lands on the thing inside the ring in one cut.
 * Returns the free text alone when nothing was marked.
 */
export function composeAnnotatedMessage(
  text: string,
  annotations: Annotation[],
  /** Masks the app already built from the drawn rings, keyed by annotation id. */
  masks: Record<string, string> = {},
): string {
  const marks = annotations.filter((a) => a.note.trim() || text.trim());
  if (!marks.length) return text.trim();

  const places = marks.map((a, i) => describe(a, i + 1, text.trim(), masks[a.id])).join('\n');
  const ready = marks.every((a) => masks[a.id]);
  const lines = [
    marks.length === 1 ? 'I marked one place on the photo:' : `I marked ${marks.length} places on the photo:`,
    places,
    '',
    ready
      ? 'Those masks are the shapes I drew, so do not cut them again: change what is inside each one with'
      : 'For a place given as coordinates (a fraction of the frame capture_mask shows, 0,0 = top-left): make a mask',
    ready
      ? 'update_mask. Leave every other part of the photo untouched.'
      : 'there with create_smart_mask, check it with capture_mask, then change only what is inside it. Leave every other part of the photo untouched.',
  ];
  const extra = text.trim() && marks.some((a) => a.note.trim()) ? `\n\nAlso: ${text.trim()}` : '';
  return lines.join('\n') + extra;
}
