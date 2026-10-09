/**
 * Converts RapidRAW adjustments into a Lightroom / Lightroom Mobile develop preset (.xmp).
 *
 * Mapping choices (agreed with the owner):
 * - Exposure: Lightroom Exposure2012 = EV Shift (`exposure`) + Exposure (`brightness`).
 * - White balance: relative IncrementalTemperature/Tint, which Lightroom applies to JPEG/HEIC only
 *   (raw photos keep their own white balance).
 * - Features Lightroom has no equivalent for are skipped and reported in `skipped`.
 * Structure follows a real Adobe Camera Raw preset (x:xmpmeta > rdf:Description with crs:* attributes).
 */

type Adj = Record<string, any>;

export interface LightroomXmpResult {
  xml: string;
  skipped: string[];
}

const HSL_COLORS: Array<[string, string]> = [
  ['reds', 'Red'],
  ['oranges', 'Orange'],
  ['yellows', 'Yellow'],
  ['greens', 'Green'],
  ['aquas', 'Aqua'],
  ['blues', 'Blue'],
  ['purples', 'Purple'],
  ['magentas', 'Magenta'],
];

const num = (v: unknown, fallback = 0) => (typeof v === 'number' && Number.isFinite(v) ? v : fallback);
const int = (v: unknown) => Math.round(num(v));
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const signed = (v: number, decimals = 0) => {
  const fixed = v.toFixed(decimals);
  return v > 0 ? `+${fixed}` : fixed;
};
const escapeXml = (s: string) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

const uuid = () =>
  Array.from(crypto.getRandomValues(new Uint8Array(16)), (b) => b.toString(16).padStart(2, '0'))
    .join('')
    .toUpperCase();

const isLinearCurve = (points: Array<{ x: number; y: number }> | undefined) =>
  !points || points.length === 0 || points.every((p) => Math.round(p.x) === Math.round(p.y));

const curveSeq = (points: Array<{ x: number; y: number }>) =>
  points.map((p) => `          <rdf:li>${clamp(int(p.x), 0, 255)}, ${clamp(int(p.y), 0, 255)}</rdf:li>`).join('\n');

export function adjustmentsToLightroomXmp(name: string, adj: Adj): LightroomXmpResult {
  const attrs: Array<[string, string | number]> = [];
  const children: string[] = [];
  const skipped: string[] = [];
  const set = (key: string, value: string | number) => attrs.push([key, value]);

  // Basic: tone and presence.
  set('Exposure2012', signed(clamp(num(adj.exposure) + num(adj.brightness), -5, 5), 2));
  set('Contrast2012', signed(int(adj.contrast)));
  set('Highlights2012', signed(int(adj.highlights)));
  set('Shadows2012', signed(int(adj.shadows)));
  set('Whites2012', signed(int(adj.whites)));
  set('Blacks2012', signed(int(adj.blacks)));
  set('Texture', signed(int(adj.structure)));
  set('Clarity2012', signed(int(adj.clarity)));
  set('Dehaze', signed(int(adj.dehaze)));
  set('Vibrance', signed(int(adj.vibrance)));
  set('Saturation', signed(int(adj.saturation)));

  // White balance: relative only (JPEG/HEIC).
  set('IncrementalTemperature', signed(int(adj.temperature)));
  set('IncrementalTint', signed(int(adj.tint)));

  // HSL / color mixer.
  for (const [rr, lr] of HSL_COLORS) {
    const c = adj.hsl?.[rr] ?? {};
    set(`HueAdjustment${lr}`, int(c.hue));
    set(`SaturationAdjustment${lr}`, int(c.saturation));
    set(`LuminanceAdjustment${lr}`, int(c.luminance));
  }

  // Color grading (Lightroom keeps the legacy split-toning names for shadow/highlight hue and saturation).
  const cg = adj.colorGrading ?? {};
  set('SplitToningShadowHue', int(cg.shadows?.hue));
  set('SplitToningShadowSaturation', int(cg.shadows?.saturation));
  set('ColorGradeShadowLum', int(cg.shadows?.luminance));
  set('ColorGradeMidtoneHue', int(cg.midtones?.hue));
  set('ColorGradeMidtoneSat', int(cg.midtones?.saturation));
  set('ColorGradeMidtoneLum', int(cg.midtones?.luminance));
  set('SplitToningHighlightHue', int(cg.highlights?.hue));
  set('SplitToningHighlightSaturation', int(cg.highlights?.saturation));
  set('ColorGradeHighlightLum', int(cg.highlights?.luminance));
  set('ColorGradeGlobalHue', int(cg.global?.hue));
  set('ColorGradeGlobalSat', int(cg.global?.saturation));
  set('ColorGradeGlobalLum', int(cg.global?.luminance));
  set('ColorGradeBlending', int(num(cg.blending, 50)));
  set('SplitToningBalance', int(cg.balance));

  // Tone curve.
  if (adj.curveMode === 'parametric') {
    const luma = adj.parametricCurve?.luma ?? {};
    set('ParametricShadows', int(luma.shadows));
    set('ParametricDarks', int(luma.darks));
    set('ParametricLights', int(luma.lights));
    set('ParametricHighlights', int(luma.highlights));
    set('ParametricShadowSplit', int(num(luma.split1, 25)));
    set('ParametricMidtoneSplit', int(num(luma.split2, 50)));
    set('ParametricHighlightSplit', int(num(luma.split3, 75)));
    if (num(luma.blackLevel) !== 0 || num(luma.whiteLevel) !== 0) skipped.push('curve black/white level');
    const rgb = ['red', 'green', 'blue'].some((ch) =>
      Object.entries(adj.parametricCurve?.[ch] ?? {}).some(([k, v]) => !k.startsWith('split') && num(v) !== 0),
    );
    if (rgb) skipped.push('red/green/blue parametric curves');
  } else {
    const curves = adj.pointCurves ?? adj.curves ?? {};
    const channels: Array<[string, string]> = [
      ['luma', 'ToneCurvePV2012'],
      ['red', 'ToneCurvePV2012Red'],
      ['green', 'ToneCurvePV2012Green'],
      ['blue', 'ToneCurvePV2012Blue'],
    ];
    for (const [ch, element] of channels) {
      const points = curves[ch];
      if (isLinearCurve(points)) continue;
      children.push(
        `      <crs:${element}>\n        <rdf:Seq>\n${curveSeq(points)}\n        </rdf:Seq>\n      </crs:${element}>`,
      );
    }
  }

  // Detail.
  const sharpness = num(adj.sharpness);
  set('Sharpness', clamp(int(sharpness), 0, 150));
  if (sharpness < 0) skipped.push('negative sharpening (softening)');
  set('LuminanceSmoothing', clamp(int(adj.lumaNoiseReduction), 0, 100));
  set('ColorNoiseReduction', clamp(int(adj.colorNoiseReduction), 0, 100));

  // Effects.
  set('PostCropVignetteAmount', signed(int(adj.vignetteAmount)));
  set('PostCropVignetteMidpoint', int(num(adj.vignetteMidpoint, 50)));
  set('PostCropVignetteRoundness', signed(int(adj.vignetteRoundness)));
  set('PostCropVignetteFeather', int(num(adj.vignetteFeather, 50)));
  set('GrainAmount', clamp(int(adj.grainAmount), 0, 100));
  set('GrainSize', clamp(int(num(adj.grainSize, 25)), 0, 100));
  set('GrainFrequency', clamp(int(num(adj.grainRoughness, 50)), 0, 100));

  // No Lightroom equivalent: report only when actually used.
  const unsupported: Array<[boolean, string]> = [
    [num(adj.glowAmount) > 0, 'glow'],
    [num(adj.halationAmount) > 0, 'halation'],
    [num(adj.flareAmount) > 0, 'light flares'],
    [!!adj.lensBlurEnabled, 'lens blur'],
    [!!adj.lutPath || !!adj.lutName, 'LUT'],
    [Array.isArray(adj.masks) && adj.masks.length > 0, 'masks'],
    [Array.isArray(adj.aiPatches) && adj.aiPatches.length > 0, 'AI patches'],
    [adj.toneMapper === 'agx', 'AgX tone mapper'],
    [num(adj['centré']) !== 0, 'centre'],
    [num(adj.chromaticAberrationRedCyan) !== 0 || num(adj.chromaticAberrationBlueYellow) !== 0, 'chromatic aberration'],
    [Object.values(adj.colorCalibration ?? {}).some((v) => num(v) !== 0), 'color calibration'],
    [num(adj.hue) !== 0, 'global hue shift'],
    [!!adj.doubleExposurePath, 'double exposure'],
    [num(adj.frameWidth) > 0 || num(adj.frameRadius) > 0, 'frame'],
    [!!adj.popBarsEnabled, '3D pop-out bars'],
    [!!adj.glassEnabled, 'fractal glass'],
    [!!adj.stretchEnabled, 'pixel stretch'],
    [!!adj.reflectEnabled, 'reflection'],
    [!!adj.motionEnabled, 'motion blur / light trails'],
  ];
  for (const [used, label] of unsupported) if (used) skipped.push(label);

  const attrLines = attrs.map(([k, v]) => `      crs:${k}="${escapeXml(String(v))}"`).join('\n');
  const xml = `<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description
      xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
      crs:PresetType="Normal"
      crs:UUID="${uuid()}"
      crs:Version="13.3"
      crs:ProcessVersion="11.0"
      crs:HasSettings="True"
${attrLines}
    >
      <crs:Name>
        <rdf:Alt>
          <rdf:li xml:lang="x-default">${escapeXml(name)}</rdf:li>
        </rdf:Alt>
      </crs:Name>
      <crs:Group>
        <rdf:Alt>
          <rdf:li xml:lang="x-default">RapidRAW Studio</rdf:li>
        </rdf:Alt>
      </crs:Group>${children.length ? '\n' + children.join('\n') : ''}
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
`;
  return { xml, skipped };
}
