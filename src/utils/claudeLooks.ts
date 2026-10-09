import {
  Aperture,
  Clapperboard,
  Film,
  FlipVertical2,
  Frame,
  Layers,
  MoveHorizontal,
  Sparkles,
  Waves,
  Wind,
} from 'lucide-react';

/**
 * One-click looks Claude builds on the open photo with real edits (masks, effects, grading).
 * Labels come from i18n (looks.<id>.label / .description); the prompt is what Claude receives.
 */
export interface ClaudeLook {
  id: string;
  icon: typeof Sparkles;
  prompt: string;
}

// Shared ending so every look is checked, reversible and reported the same way.
const FINISH = (versionName: string) =>
  ` Look at the result with capture_preview and refine up to 2 times: keep the photo believable, ` +
  `shadows with detail, nothing clipped. Then save_version named "${versionName}" and tell me in one ` +
  `line what you did.`;

export const CLAUDE_LOOKS: ClaudeLook[] = [
  {
    id: 'fractalGlass',
    icon: Layers,
    prompt:
      'Give this photo the viral fractal glass look. Look at it with capture_preview, then set fractal glass ' +
      '(glassEnabled, glassWidth 2-5, glassStrength 50-80, glassHighlight 20-35, a little glassCurve with ' +
      'glassCurveWaves 1-2 if it suits the photo). Place the glass box (glassStart/glassBottom/glassLeft/glassRight) ' +
      'so it crosses the body or background but leaves the face or main point of interest clear.' +
      FINISH('Fractal glass'),
  },
  {
    id: 'popOut',
    icon: MoveHorizontal,
    prompt:
      'Make a 3D pop-out edit: create a smart mask of the main subject (check it with capture_mask and fix it ' +
      'until it is tight), then enable popBarsEnabled with popBarsMaskId set to that mask. Pick popBarsOrientation, ' +
      'popBarsWidth and popBarsInset so part of the subject clearly overlaps a bar and appears to break out of the frame.' +
      FINISH('3D pop-out'),
  },
  {
    id: 'silkyWater',
    icon: Waves,
    prompt:
      'Give the water a long-exposure silky look: create a smart mask of all the water (check it with capture_mask ' +
      'so nothing else is included), then set motionEnabled with motionMode "water", motionMaskId set to that mask, ' +
      'motionAngle following the water flow and motionLength 3-8. If there is no water in the photo, say so and stop.' +
      FINISH('Silky water'),
  },
  {
    id: 'lightTrails',
    icon: Sparkles,
    prompt:
      'Add long-exposure light trails: set motionEnabled with motionMode "trails", motionAngle along the natural ' +
      'movement in the scene (traffic, a pan, or horizontal), motionLength 8-16, and motionThreshold high enough that ' +
      'only real lights streak, not bright sky or snow. Works best on night photos; if the photo has no distinct ' +
      'lights, say so and stop.' +
      FINISH('Light trails'),
  },
  {
    id: 'movementBlur',
    icon: Wind,
    prompt:
      'Make a motion-blur street-style edit: create a smart mask of the main subject, then set motionEnabled with ' +
      'motionMode "movement", motionMaskId set to that mask so the subject stays sharp, motionAngle along the ' +
      "subject's direction of travel and motionLength 5-10." +
      FINISH('Movement blur'),
  },
  {
    id: 'reflection',
    icon: FlipVertical2,
    prompt:
      'Add a mirror reflection: choose a natural waterline (the bottom of a building, a horizon, a shoreline) and set ' +
      'reflectEnabled with reflectLine there, reflectRipple 10-30 and reflectFade 20-40. If the scene has water, limit ' +
      'it with a smart water mask via reflectMaskId.' +
      FINISH('Reflection'),
  },
  {
    id: 'pixelStretch',
    icon: Aperture,
    prompt:
      'Make a pixel-stretch edit: pick a strong edge of the main subject and set stretchEnabled with stretchPosition ' +
      'at that edge, stretchDirection away from the subject, and stretchBandStart/stretchBandEnd covering a band ' +
      'that crosses an interesting part of it, so the colours streak out to the edge of the frame.' +
      FINISH('Pixel stretch'),
  },
  {
    id: 'cinematic',
    icon: Clapperboard,
    prompt:
      'Give this photo a cinematic grade with colour and light only, no creative effects: warm highlights, cooler ' +
      'shadows, gentle contrast, slightly lifted blacks and restrained saturation. Keep vignette gentle.' +
      FINISH('Cinematic'),
  },
  {
    id: 'film',
    icon: Film,
    prompt:
      'Give this photo an analogue film look with real edits only: soft contrast, faded blacks, a slight warm or ' +
      'green cast depending on the scene, natural film grain (grainAmount 20-40) and a subtle vignette.' +
      FINISH('Film'),
  },
  {
    id: 'instagramFrame',
    icon: Frame,
    prompt:
      'Prepare this photo for an Instagram post: a clean, slightly bright grade that flatters the subject, then a ' +
      'thin frame (frameWidth 2-4, frameRadius 1-3) in white, or a colour picked from the photo if it suits it better.' +
      FINISH('Instagram frame'),
  },
];
