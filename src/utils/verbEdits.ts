/**
 * Plain edits the app can make by itself.
 *
 * Most of what anyone asks of a marked area is one of a dozen verbs: darker, warmer, punchier. Sending those to
 * a model costs tokens and a wait, and a table of slider deltas is both cheaper and steadier -- it gives the same
 * answer every time. Anything with a condition, a comparison or an idea in it ("cinematic", "keep the sky
 * honest", "like the other photo") does not match here and goes to the assistant, which is what it is for.
 */

/** The keys a verb may move. Masks carry the same ones as the photo, so a verb works on either. */
export type VerbDeltas = Partial<
  Record<
    | 'exposure'
    | 'contrast'
    | 'highlights'
    | 'shadows'
    | 'whites'
    | 'blacks'
    | 'saturation'
    | 'vibrance'
    | 'temperature'
    | 'clarity'
    | 'structure'
    | 'dehaze'
    | 'sharpness',
    number
  >
>;

/** How far each slider may go, so repeated nudges stop at the end instead of running past it. */
const LIMITS: Record<string, [number, number]> = {
  exposure: [-5, 5],
  contrast: [-100, 100],
  highlights: [-100, 100],
  shadows: [-100, 100],
  whites: [-100, 100],
  blacks: [-100, 100],
  saturation: [-100, 100],
  vibrance: [-100, 100],
  temperature: [-100, 100],
  clarity: [-100, 100],
  structure: [-100, 100],
  dehaze: [-100, 100],
  sharpness: [0, 100],
};

interface Verb {
  /** Matched against the instruction with nothing but filler around it. */
  words: RegExp;
  /** What it means, shown back to the user. */
  label: string;
  deltas: VerbDeltas;
}

// One step of each verb. Modest on purpose: a nudge you repeat beats a leap you undo.
const VERBS: Verb[] = [
  { words: /darker|darken/, label: 'darker', deltas: { exposure: -0.35, blacks: -6 } },
  { words: /brighter|brighten|lighter|lighten/, label: 'brighter', deltas: { exposure: 0.35 } },
  { words: /warmer|warm up/, label: 'warmer', deltas: { temperature: 12 } },
  { words: /cooler|cool down/, label: 'cooler', deltas: { temperature: -12 } },
  { words: /punchier|punchy|more contrast|contrastier/, label: 'punchier', deltas: { contrast: 18, clarity: 6 } },
  { words: /flatter|less contrast/, label: 'softer', deltas: { contrast: -15, clarity: -6 } },
  {
    words: /more saturated|richer|more colou?rful/,
    label: 'richer in colour',
    deltas: { saturation: 12, vibrance: 8 },
  },
  {
    words: /less saturated|desaturated|desaturate|muted|duller/,
    label: 'less saturated',
    deltas: { saturation: -18, vibrance: -6 },
  },
  { words: /sharper|sharpen|crisper/, label: 'sharper', deltas: { sharpness: 18, structure: 8 } },
  { words: /dreamier|hazier/, label: 'dreamier', deltas: { clarity: -18, dehaze: -8 } },
  { words: /clearer|less haze|dehaze/, label: 'clearer', deltas: { dehaze: 18, clarity: 5 } },
  {
    words: /lift the shadows|lift shadows|open the shadows|brighter shadows/,
    label: 'lifted in the shadows',
    deltas: { shadows: 15 },
  },
  {
    words: /deepen the shadows|deeper shadows|crush the shadows|darker shadows/,
    label: 'deeper in the shadows',
    deltas: { shadows: -12, blacks: -10 },
  },
  {
    words: /recover the highlights|pull the highlights|tame the highlights|darker highlights/,
    label: 'gentler in the highlights',
    deltas: { highlights: -18 },
  },
  { words: /brighter highlights|lift the highlights/, label: 'brighter in the highlights', deltas: { highlights: 15 } },
];

// "a touch darker", "much darker": how hard the nudge lands.
const GENTLE = /(slightly|a little|a bit|a touch|a tad|gently|subtly|marginally)/;
const STRONG = /(much|a lot|lots|really|way|far|significantly|considerably|strongly)/;

// Words that carry no instruction of their own, so they may sit around a verb.
const FILLER = new RegExp(
  '\\b(' +
    [
      'please', 'can', 'you', 'could', 'make', 'makes', 'made', 'making', 'it', 'this', 'that', 'these', 'those',
      'here', 'there', 'the', 'a', 'an', 'is', 'be', 'look', 'looks', 'looking', 'more', 'bit', 'little', 'touch',
      'tad', 'slightly', 'subtly', 'gently', 'much', 'lot', 'lots', 'really', 'way', 'far', 'significantly',
      'considerably', 'strongly', 'marginally', 'and', 'just', 'area', 'areas', 'part', 'region', 'photo', 'image',
      'picture', 'crop', 'one', 'up', 'down',
    ].join('|') +
    ')\\b',
  'g',
);

// A condition, a comparison or a second thought: never a plain nudge, whatever verb it contains.
const NOT_PLAIN = new RegExp(
  '\\b(but|except|without|unless|while|whilst|keep|keeping|like|match|instead|then|if|only|than|so)\\b',
);

export interface VerbEdit {
  label: string;
  deltas: VerbDeltas;
}

/**
 * The edit an instruction plainly asks for, or null when it asks for anything more than that -- a condition, a
 * comparison, an idea, or two things at once. Null means "the assistant should read this", and that is the safe
 * direction to fail in: doing nothing costs a moment, doing the wrong thing costs the photo.
 */
export function parseVerbEdit(instruction: string): VerbEdit | null {
  const text = instruction.toLowerCase().trim().replace(/[.!,;:]+$/g, '');
  if (!text || NOT_PLAIN.test(text)) return null;

  const hits = VERBS.filter((v) => v.words.test(text));
  // Two verbs at once is a request with a shape to it; let the assistant weigh them.
  if (hits.length !== 1) return null;
  const verb = hits[0];

  // Everything the verb did not claim must be filler, or this says more than the table knows.
  const leftover = text.replace(verb.words, ' ').replace(FILLER, ' ').replace(/[^a-z]+/g, ' ').trim();
  if (leftover) return null;

  const scale = STRONG.test(text) ? 1.8 : GENTLE.test(text) ? 0.5 : 1;
  const deltas: VerbDeltas = {};
  for (const [key, value] of Object.entries(verb.deltas)) {
    deltas[key as keyof VerbDeltas] = Math.round(value * scale * 100) / 100;
  }
  const label = scale > 1 ? `much ${verb.label}` : scale < 1 ? `slightly ${verb.label}` : verb.label;
  return { label, deltas };
}

/** The deltas added to what is already set, each kept inside its slider's range. */
export function applyVerbEdit<T extends Record<string, unknown>>(current: T, deltas: VerbDeltas): T {
  const next: Record<string, unknown> = { ...current };
  for (const [key, delta] of Object.entries(deltas)) {
    const [min, max] = LIMITS[key] ?? [-100, 100];
    const from = typeof current[key] === 'number' ? current[key] : 0;
    next[key] = Math.min(max, Math.max(min, Math.round((from + (delta ?? 0)) * 100) / 100));
  }
  return next as T;
}
