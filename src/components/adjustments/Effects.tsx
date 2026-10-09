import { useState, useEffect, useRef, useMemo, useCallback } from 'react';
import Dropdown from '../ui/Dropdown';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { Loader2, Circle, Hexagon, Octagon, Aperture } from 'lucide-react';
import { motion } from 'framer-motion';
import clsx from 'clsx';
import Slider from '../ui/Slider';
import Switch from '../ui/Switch';
import { Adjustments, BLEND_MODES, Effect, CreativeAdjustment } from '../../utils/adjustments';
import { AppSettings } from '../ui/AppProperties';
import Text from '../ui/Text';
import { TextVariants } from '../../types/typography';
import { DepthRangePicker } from '../ui/DepthRangePicker';
import { useProcessStore } from '../../store/useProcessStore';
import { useEditorStore } from '../../store/useEditorStore';

// Frame colours worth one click; anything else comes from the picker beside them.
const FRAME_COLORS = [
  { value: 'white', css: '#ffffff', labelKey: 'frame.white' },
  { value: 'black', css: '#000000', labelKey: 'frame.black' },
  { value: '#f2ece1', css: '#f2ece1', labelKey: 'frame.ivory' },
  { value: '#d9d9d9', css: '#d9d9d9', labelKey: 'frame.silver' },
  { value: '#1f1f1f', css: '#1f1f1f', labelKey: 'frame.charcoal' },
  { value: '#2b2118', css: '#2b2118', labelKey: 'frame.espresso' },
];

interface EffectsPanelProps {
  adjustments: Adjustments;
  isForMask?: boolean;
  setAdjustments(adjustments: Partial<Adjustments> | ((prev: Adjustments) => Adjustments)): any;
  appSettings: AppSettings | null;
  onDragStateChange?: (isDragging: boolean) => void;
}

interface BokehShapeSwitchProps {
  selectedShape: string;
  onShapeChange: (shape: string) => void;
}

const BokehShapeSwitch = ({ selectedShape, onShapeChange }: BokehShapeSwitchProps) => {
  const { t } = useTranslation();
  const [bubbleStyle, setBubbleStyle] = useState({});
  const [isLabelHovered, setIsLabelHovered] = useState(false);
  const isInitialAnimation = useRef(true);

  const shapeOptions = useMemo(
    () => [
      { id: 'circle', icon: Circle, title: t('adjustments.effects.bokehCircular') },
      { id: 'hexagon', icon: Hexagon, title: t('adjustments.effects.bokehHexagonal') },
      { id: 'octagon', icon: Octagon, title: t('adjustments.effects.bokehOctagonal') },
      { id: 'ring', icon: Aperture, title: t('adjustments.effects.bokehRing') },
    ],
    [t],
  );

  useEffect(() => {
    const selectedIndex = shapeOptions.findIndex((m) => m.id === selectedShape);
    const safeIndex = selectedIndex >= 0 ? selectedIndex : 0;

    const widthPercent = 100 / shapeOptions.length;
    const targetX = `${safeIndex * 100}%`;
    const targetWidth = `${widthPercent}%`;

    if (isInitialAnimation.current) {
      setBubbleStyle({
        x: ['-25%', targetX],
        width: targetWidth,
      });
      isInitialAnimation.current = false;
    } else {
      setBubbleStyle({
        x: targetX,
        width: targetWidth,
      });
    }
  }, [selectedShape, shapeOptions]);

  const handleReset = () => {
    onShapeChange('circle');
  };

  return (
    <div className="flex flex-col gap-2 mt-3">
      <div
        className="grid w-fit cursor-pointer"
        onClick={handleReset}
        onMouseEnter={() => setIsLabelHovered(true)}
        onMouseLeave={() => setIsLabelHovered(false)}
      >
        <Text
          variant={TextVariants.label}
          aria-hidden={isLabelHovered}
          className={`col-start-1 row-start-1 text-text-secondary select-none transition-opacity duration-200 ease-in-out ${
            isLabelHovered ? 'opacity-0' : 'opacity-100'
          }`}
        >
          {t('adjustments.effects.bokehShape')}
        </Text>
        <Text
          variant={TextVariants.label}
          aria-hidden={!isLabelHovered}
          className={`col-start-1 row-start-1 text-accent! select-none transition-opacity duration-200 ease-in-out pointer-events-none ${
            isLabelHovered ? 'opacity-100' : 'opacity-0'
          }`}
        >
          {t('ui.slider.reset')}
        </Text>
      </div>

      <div className="w-full p-1 bg-bg-primary rounded-md">
        <div className="relative flex w-full">
          <motion.div
            className="absolute top-0 bottom-0 z-0 bg-accent"
            style={{ borderRadius: 6 }}
            animate={bubbleStyle}
            transition={{ type: 'spring', bounce: 0.2, duration: 0.6 }}
          />
          {shapeOptions.map((shape) => {
            const Icon = shape.icon;
            return (
              <button
                key={shape.id}
                data-tooltip={shape.title}
                onClick={() => onShapeChange(shape.id)}
                className={clsx(
                  'relative flex-1 flex items-center justify-center gap-2 px-3 py-1.5 text-sm font-medium rounded-md transition-colors',
                  {
                    'text-text-secondary hover:text-text-primary hover:bg-surface': selectedShape !== shape.id,
                    'text-button-text': selectedShape === shape.id,
                  },
                )}
                style={{ WebkitTapHighlightColor: 'transparent' }}
              >
                <span className="relative z-10 flex items-center">
                  <Icon size={16} strokeWidth={2} />
                </span>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
};

export default function EffectsPanel({
  adjustments,
  setAdjustments,
  isForMask = false,
  appSettings,
  onDragStateChange,
}: EffectsPanelProps) {
  const { t } = useTranslation();
  const isMovingDoubleExposure = useEditorStore((state) => state.isMovingDoubleExposure);
  const effectGuide = useEditorStore((state) => state.effectGuide);
  const [isGeneratingDepth, setIsGeneratingDepth] = useState(false);
  const aiModelDownloadStatus = useProcessStore((state) => state.aiModelDownloadStatus);

  const handleGenerateLensBlurDepthMap = async () => {
    setIsGeneratingDepth(true);
    try {
      const b64: string = await invoke('generate_full_image_depth_map', { jsAdjustments: adjustments });
      setAdjustments((prev: Partial<Adjustments>) => ({
        ...prev,
        lensBlurDepthMap: b64,
      }));
    } catch (e: any) {
      toast.error(`Failed to generate depth map: ${e}`);
      setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, lensBlurEnabled: false }));
    } finally {
      setIsGeneratingDepth(false);
    }
  };

  const handleAdjustmentChange = (key: string, value: any) => {
    const numericValue = typeof value === 'boolean' ? value : parseInt(value, 10);
    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, [key]: numericValue }));
  };

  const handleLensBlurToggle = (enabled: boolean) => {
    handleAdjustmentChange(Effect.LensBlurEnabled, enabled);
    if (enabled && !adjustments.lensBlurDepthMap) {
      handleGenerateLensBlurDepthMap();
    }
  };

  const adjustmentVisibility = appSettings?.adjustmentVisibility || {};


  // Hovering a blend mode shows it on the photo; leaving the menu without choosing puts the old one back. Written
  // straight to the store, so only the mode actually chosen becomes a history step.
  const blendPreviewBase = useRef<string | null>(null);
  const previewBlendMode = useCallback((mode: string | null) => {
    const { adjustments: current, setEditor } = useEditorStore.getState();
    if (mode === null && blendPreviewBase.current === null) return;
    if (mode !== null && blendPreviewBase.current === null) {
      blendPreviewBase.current = current.doubleExposureMode || 'normal';
    }
    const shown = mode ?? blendPreviewBase.current;
    if (mode === null) blendPreviewBase.current = null;
    setEditor((state) => ({ adjustments: { ...state.adjustments, doubleExposureMode: shown } }));
  }, []);
  return (
    <div className="space-y-2.5">
      {!isForMask && (
        <div className="space-y-2.5">
          {adjustmentVisibility.vignette !== false && (
            <div className="p-2 bg-bg-tertiary rounded-md">
              <Text variant={TextVariants.heading} className="mb-2">
                {t('adjustments.effects.vignette')}
              </Text>
              <Slider
                label={t('adjustments.effects.amount')}
                max={100}
                min={-100}
                onChange={(e: any) => handleAdjustmentChange(Effect.VignetteAmount, e.target.value)}
                step={1}
                value={adjustments.vignetteAmount}
                onDragStateChange={onDragStateChange}
              />
              <Slider
                defaultValue={50}
                label={t('adjustments.effects.midpoint')}
                max={100}
                min={0}
                onChange={(e: any) => handleAdjustmentChange(Effect.VignetteMidpoint, e.target.value)}
                step={1}
                value={adjustments.vignetteMidpoint}
                onDragStateChange={onDragStateChange}
                fillOrigin="min"
              />
              <Slider
                label={t('adjustments.effects.roundness')}
                max={100}
                min={-100}
                onChange={(e: any) => handleAdjustmentChange(Effect.VignetteRoundness, e.target.value)}
                step={1}
                value={adjustments.vignetteRoundness}
                onDragStateChange={onDragStateChange}
              />
              <Slider
                defaultValue={50}
                label={t('adjustments.effects.feather')}
                max={100}
                min={0}
                onChange={(e: any) => handleAdjustmentChange(Effect.VignetteFeather, e.target.value)}
                step={1}
                value={adjustments.vignetteFeather}
                onDragStateChange={onDragStateChange}
                fillOrigin="min"
              />
            </div>
          )}

          {adjustmentVisibility.grain !== false && (
            <div className="p-2 bg-bg-tertiary rounded-md">
              <Text variant={TextVariants.heading} className="mb-2">
                {t('adjustments.effects.grain')}
              </Text>
              <Slider
                label={t('adjustments.effects.amount')}
                max={100}
                min={0}
                onChange={(e: any) => handleAdjustmentChange(Effect.GrainAmount, e.target.value)}
                step={1}
                value={adjustments.grainAmount}
                onDragStateChange={onDragStateChange}
              />
              <Slider
                defaultValue={25}
                label={t('adjustments.effects.size')}
                max={100}
                min={0}
                onChange={(e: any) => handleAdjustmentChange(Effect.GrainSize, e.target.value)}
                step={1}
                value={adjustments.grainSize}
                onDragStateChange={onDragStateChange}
                fillOrigin="min"
              />
              <Slider
                defaultValue={50}
                label={t('adjustments.effects.roughness')}
                max={100}
                min={0}
                onChange={(e: any) => handleAdjustmentChange(Effect.GrainRoughness, e.target.value)}
                step={1}
                value={adjustments.grainRoughness}
                onDragStateChange={onDragStateChange}
                fillOrigin="min"
              />
            </div>
          )}

          <div className="p-2 bg-bg-tertiary rounded-md">
            <Text variant={TextVariants.heading} className="mb-2">
              {t('doubleExposure.title')}
            </Text>
            <div className="flex items-center gap-2 mb-2">
              <button
                className="flex-1 truncate text-left px-3 py-2 rounded-md bg-bg-secondary text-sm text-text-primary hover:bg-card-active"
                onClick={async () => {
                  const picked = await openDialog({
                    multiple: false,
                    filters: [
                      {
                        name: 'Images',
                        extensions: [
                          'jpg',
                          'jpeg',
                          'png',
                          'tif',
                          'tiff',
                          'webp',
                          'dng',
                          'cr2',
                          'cr3',
                          'nef',
                          'arw',
                          'raf',
                          'orf',
                          'rw2',
                        ],
                      },
                    ],
                  });
                  if (typeof picked === 'string')
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, doubleExposurePath: picked }));
                }}
              >
                {adjustments.doubleExposurePath
                  ? adjustments.doubleExposurePath.split(/[/\\]/).pop()
                  : t('doubleExposure.choose')}
              </button>
              {adjustments.doubleExposurePath && (
                <button
                  className="px-2 py-2 rounded-md text-text-secondary hover:text-text-primary hover:bg-surface text-sm"
                  onClick={() =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, doubleExposurePath: null }))
                  }
                >
                  {t('doubleExposure.clear')}
                </button>
              )}
            </div>
            {adjustments.doubleExposurePath && (
              <>
                <Dropdown
                  className="w-full mb-2"
                  options={[
                    { label: t('doubleExposure.wholePhoto'), value: '' },
                    ...(adjustments.masks || []).map((m: any) => ({ label: m.name, value: m.id })),
                  ]}
                  value={adjustments.doubleExposureMaskId || ''}
                  onChange={(maskId: string) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, doubleExposureMaskId: maskId || null }))
                  }
                />
                <Dropdown
                  className="w-full mb-2"
                  options={BLEND_MODES.map((mode) => ({
                    label: t(`doubleExposure.modes.${mode}`),
                    value: mode,
                  }))}
                  value={adjustments.doubleExposureMode || 'normal'}
                  onChange={(mode: string) => {
                    blendPreviewBase.current = null;
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, doubleExposureMode: mode }));
                  }}
                  onHighlight={previewBlendMode}
                />
                <Slider
                  defaultValue={50}
                  label={t('doubleExposure.opacity')}
                  max={100}
                  min={0}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({
                      ...prev,
                      doubleExposureOpacity: Number(e.target.value),
                    }))
                  }
                  step={1}
                  value={adjustments.doubleExposureOpacity ?? 50}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={100}
                  label={t('doubleExposure.scale')}
                  max={300}
                  min={25}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({
                      ...prev,
                      doubleExposureScale: Number(e.target.value),
                    }))
                  }
                  step={1}
                  value={adjustments.doubleExposureScale ?? 100}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={0}
                  label={t('doubleExposure.moveX')}
                  max={100}
                  min={-100}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({
                      ...prev,
                      doubleExposureOffsetX: Number(e.target.value),
                    }))
                  }
                  step={1}
                  value={adjustments.doubleExposureOffsetX ?? 0}
                  onDragStateChange={onDragStateChange}
                />
                <Slider
                  defaultValue={0}
                  label={t('doubleExposure.moveY')}
                  max={100}
                  min={-100}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({
                      ...prev,
                      doubleExposureOffsetY: Number(e.target.value),
                    }))
                  }
                  step={1}
                  value={adjustments.doubleExposureOffsetY ?? 0}
                  onDragStateChange={onDragStateChange}
                />
                <button
                  className={clsx(
                    'w-full mt-2 px-3 py-2 rounded-md text-sm',
                    isMovingDoubleExposure
                      ? 'bg-accent text-button-text'
                      : 'bg-bg-secondary text-text-primary hover:bg-card-active',
                  )}
                  onClick={() =>
                    useEditorStore.getState().setEditor({ isMovingDoubleExposure: !isMovingDoubleExposure })
                  }
                >
                  {t(isMovingDoubleExposure ? 'doubleExposure.moveDone' : 'doubleExposure.moveOnPhoto')}
                </button>
                {isMovingDoubleExposure && (
                  <Text variant={TextVariants.small} className="mt-1 block text-center">
                    {t('doubleExposure.moveHint')}
                  </Text>
                )}
              </>
            )}
          </div>

          <div className="p-2 bg-bg-tertiary rounded-md">
            <Text variant={TextVariants.heading} className="mb-2">
              {t('frame.title')}
            </Text>
            <Slider
              defaultValue={0}
              label={t('frame.width')}
              max={20}
              min={0}
              onChange={(e: any) =>
                setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, frameWidth: Number(e.target.value) }))
              }
              step={0.5}
              value={adjustments.frameWidth ?? 0}
              onDragStateChange={onDragStateChange}
              fillOrigin="min"
            />
            <Slider
              defaultValue={0}
              label={t('frame.radius')}
              max={25}
              min={0}
              onChange={(e: any) =>
                setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, frameRadius: Number(e.target.value) }))
              }
              step={0.5}
              value={adjustments.frameRadius ?? 0}
              onDragStateChange={onDragStateChange}
              fillOrigin="min"
            />
            <div className="flex items-center gap-1.5 flex-wrap">
              {FRAME_COLORS.map((swatch) => {
                const current = (adjustments.frameColor || 'white').toLowerCase();
                const isActive = current === swatch.value || (swatch.value === 'white' && current === '#ffffff');
                return (
                  <button
                    key={swatch.value}
                    className={clsx(
                      'w-6 h-6 rounded-full border transition-transform',
                      isActive ? 'border-accent scale-110' : 'border-border-color hover:scale-105',
                    )}
                    style={{ backgroundColor: swatch.css }}
                    data-tooltip={t(swatch.labelKey as any)}
                    onClick={() =>
                      setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, frameColor: swatch.value }))
                    }
                  />
                );
              })}
              <label
                className="w-6 h-6 rounded-full border border-border-color overflow-hidden relative cursor-pointer"
                data-tooltip={t('frame.custom')}
                style={{
                  background: 'conic-gradient(#ef4444, #eab308, #22c55e, #06b6d4, #3b82f6, #a855f7, #ef4444)',
                }}
              >
                <input
                  className="opacity-0 absolute inset-0 cursor-pointer"
                  type="color"
                  value={
                    (adjustments.frameColor || '').startsWith('#')
                      ? adjustments.frameColor
                      : adjustments.frameColor === 'black'
                        ? '#000000'
                        : '#ffffff'
                  }
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, frameColor: e.target.value }))
                  }
                />
              </label>
            </div>
          </div>

          <div className="p-2 bg-bg-tertiary rounded-md">
            <div className="mb-2">
              <Switch
                label={t('popOut.title')}
                checked={!!adjustments.popBarsEnabled}
                onChange={(checked: boolean) =>
                  setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, popBarsEnabled: checked }))
                }
              />
            </div>
            {adjustments.popBarsEnabled && (
              <>
                <Text className="mb-2">{t('popOut.hint')}</Text>
                <Dropdown
                  className="w-full mb-2"
                  options={[
                    { label: t('popOut.noFront'), value: '' },
                    ...(adjustments.masks || []).map((m: any) => ({ label: m.name, value: m.id })),
                  ]}
                  value={adjustments.popBarsMaskId || ''}
                  onChange={(maskId: string) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, popBarsMaskId: maskId || null }))
                  }
                />
                <Dropdown
                  className="w-full mb-2"
                  options={[
                    { label: t('popOut.vertical'), value: 'vertical' },
                    { label: t('popOut.horizontal'), value: 'horizontal' },
                  ]}
                  value={adjustments.popBarsOrientation || 'vertical'}
                  onChange={(o: string) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, popBarsOrientation: o }))
                  }
                />
                <Slider
                  defaultValue={6}
                  label={t('popOut.width')}
                  max={25}
                  min={1}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, popBarsWidth: Number(e.target.value) }))
                  }
                  step={0.5}
                  value={adjustments.popBarsWidth ?? 6}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={18}
                  label={t('popOut.inset')}
                  max={45}
                  min={0}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, popBarsInset: Number(e.target.value) }))
                  }
                  step={0.5}
                  value={adjustments.popBarsInset ?? 18}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
              </>
            )}
          </div>

          <div className="p-2 bg-bg-tertiary rounded-md">
            <div className="mb-2">
              <Switch
                label={t('glass.title')}
                checked={!!adjustments.glassEnabled}
                onChange={(checked: boolean) =>
                  setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, glassEnabled: checked }))
                }
              />
            </div>
            {adjustments.glassEnabled && (
              <>
                <Dropdown
                  className="w-full mb-2"
                  options={[
                    { label: t('effectsRegion.wholePhoto'), value: '' },
                    ...(adjustments.masks || []).map((m: any) => ({ label: m.name, value: m.id })),
                  ]}
                  value={adjustments.glassMaskId || ''}
                  onChange={(maskId: string) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, glassMaskId: maskId || null }))
                  }
                />
                <button
                  className={clsx(
                    'w-full mb-2 px-3 py-1.5 rounded-md text-xs transition-colors',
                    effectGuide === 'glass'
                      ? 'bg-accent text-button-text'
                      : 'bg-bg-secondary text-text-primary hover:bg-card-active',
                  )}
                  onClick={() =>
                    useEditorStore.getState().setEditor({ effectGuide: effectGuide === 'glass' ? null : 'glass' })
                  }
                >
                  {t(effectGuide === 'glass' ? 'effectsRegion.guideDone' : 'effectsRegion.guideOnPhoto')}
                </button>
                <Slider
                  defaultValue={4}
                  label={t('glass.width')}
                  max={15}
                  min={1}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, glassWidth: Number(e.target.value) }))
                  }
                  step={0.5}
                  value={adjustments.glassWidth ?? 4}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={60}
                  label={t('glass.strength')}
                  max={100}
                  min={0}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, glassStrength: Number(e.target.value) }))
                  }
                  step={1}
                  value={adjustments.glassStrength ?? 60}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={0}
                  label={t('glass.angle')}
                  max={90}
                  min={-90}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, glassAngle: Number(e.target.value) }))
                  }
                  step={1}
                  value={adjustments.glassAngle ?? 0}
                  onDragStateChange={onDragStateChange}
                />
                <Slider
                  defaultValue={0}
                  label={t('glass.start')}
                  max={95}
                  min={0}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, glassStart: Number(e.target.value) }))
                  }
                  step={1}
                  value={adjustments.glassStart ?? 0}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={100}
                  label={t('glass.bottom')}
                  max={100}
                  min={5}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, glassBottom: Number(e.target.value) }))
                  }
                  step={0.5}
                  value={adjustments.glassBottom ?? 100}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={0}
                  label={t('glass.left')}
                  max={95}
                  min={0}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, glassLeft: Number(e.target.value) }))
                  }
                  step={0.5}
                  value={adjustments.glassLeft ?? 0}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={100}
                  label={t('glass.right')}
                  max={100}
                  min={5}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, glassRight: Number(e.target.value) }))
                  }
                  step={0.5}
                  value={adjustments.glassRight ?? 100}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={8}
                  label={t('glass.softness')}
                  max={50}
                  min={0}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, glassSoftness: Number(e.target.value) }))
                  }
                  step={1}
                  value={adjustments.glassSoftness ?? 8}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={25}
                  label={t('glass.highlight')}
                  max={100}
                  min={0}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({
                      ...prev,
                      glassHighlight: Number(e.target.value),
                    }))
                  }
                  step={1}
                  value={adjustments.glassHighlight ?? 25}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={0}
                  label={t('glass.curve')}
                  max={100}
                  min={0}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, glassCurve: Number(e.target.value) }))
                  }
                  step={1}
                  value={adjustments.glassCurve ?? 0}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={2}
                  label={t('glass.curveWaves')}
                  max={10}
                  min={1}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({
                      ...prev,
                      glassCurveWaves: Number(e.target.value),
                    }))
                  }
                  step={0.5}
                  value={adjustments.glassCurveWaves ?? 2}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Text variant={TextVariants.small} className="mt-1 block">
                  {t('glass.hint')}
                </Text>
              </>
            )}
          </div>

          <div className="p-2 bg-bg-tertiary rounded-md">
            <div className="mb-2">
              <Switch
                label={t('stretch.title')}
                checked={!!adjustments.stretchEnabled}
                onChange={(checked: boolean) =>
                  setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, stretchEnabled: checked }))
                }
              />
            </div>
            {adjustments.stretchEnabled && (
              <>
                <Dropdown
                  className="w-full mb-2"
                  options={[
                    { label: t('effectsRegion.wholePhoto'), value: '' },
                    ...(adjustments.masks || []).map((m: any) => ({ label: m.name, value: m.id })),
                  ]}
                  value={adjustments.stretchMaskId || ''}
                  onChange={(maskId: string) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, stretchMaskId: maskId || null }))
                  }
                />
                <button
                  className={clsx(
                    'w-full mb-2 px-3 py-1.5 rounded-md text-xs transition-colors',
                    effectGuide === 'stretch'
                      ? 'bg-accent text-button-text'
                      : 'bg-bg-secondary text-text-primary hover:bg-card-active',
                  )}
                  onClick={() =>
                    useEditorStore.getState().setEditor({ effectGuide: effectGuide === 'stretch' ? null : 'stretch' })
                  }
                >
                  {t(effectGuide === 'stretch' ? 'effectsRegion.guideDone' : 'effectsRegion.guideOnPhoto')}
                </button>
                <Dropdown
                  className="w-full mb-2"
                  options={[
                    { label: t('stretch.horizontal'), value: 'horizontal' },
                    { label: t('stretch.vertical'), value: 'vertical' },
                  ]}
                  value={adjustments.stretchAxis || 'horizontal'}
                  onChange={(value: string) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, stretchAxis: value }))
                  }
                />
                <Dropdown
                  className="w-full mb-2"
                  options={[
                    { label: t('stretch.after'), value: 'after' },
                    { label: t('stretch.before'), value: 'before' },
                  ]}
                  value={adjustments.stretchDirection || 'after'}
                  onChange={(value: string) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, stretchDirection: value }))
                  }
                />
                <Slider
                  defaultValue={50}
                  label={t('stretch.position')}
                  max={100}
                  min={0}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({
                      ...prev,
                      stretchPosition: Number(e.target.value),
                    }))
                  }
                  step={0.5}
                  value={adjustments.stretchPosition ?? 50}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={30}
                  label={t('stretch.bandStart')}
                  max={100}
                  min={0}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({
                      ...prev,
                      stretchBandStart: Number(e.target.value),
                    }))
                  }
                  step={0.5}
                  value={adjustments.stretchBandStart ?? 30}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={70}
                  label={t('stretch.bandEnd')}
                  max={100}
                  min={0}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({
                      ...prev,
                      stretchBandEnd: Number(e.target.value),
                    }))
                  }
                  step={0.5}
                  value={adjustments.stretchBandEnd ?? 70}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
              </>
            )}
          </div>

          <div className="p-2 bg-bg-tertiary rounded-md">
            <div className="mb-2">
              <Switch
                label={t('reflect.title')}
                checked={!!adjustments.reflectEnabled}
                onChange={(checked: boolean) =>
                  setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, reflectEnabled: checked }))
                }
              />
            </div>
            {adjustments.reflectEnabled && (
              <>
                <Dropdown
                  className="w-full mb-2"
                  options={[
                    { label: t('effectsRegion.wholePhoto'), value: '' },
                    ...(adjustments.masks || []).map((m: any) => ({ label: m.name, value: m.id })),
                  ]}
                  value={adjustments.reflectMaskId || ''}
                  onChange={(maskId: string) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, reflectMaskId: maskId || null }))
                  }
                />
                <button
                  className={clsx(
                    'w-full mb-2 px-3 py-1.5 rounded-md text-xs transition-colors',
                    effectGuide === 'reflect'
                      ? 'bg-accent text-button-text'
                      : 'bg-bg-secondary text-text-primary hover:bg-card-active',
                  )}
                  onClick={() =>
                    useEditorStore.getState().setEditor({ effectGuide: effectGuide === 'reflect' ? null : 'reflect' })
                  }
                >
                  {t(effectGuide === 'reflect' ? 'effectsRegion.guideDone' : 'effectsRegion.guideOnPhoto')}
                </button>
                <Slider
                  defaultValue={60}
                  label={t('reflect.line')}
                  max={95}
                  min={5}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, reflectLine: Number(e.target.value) }))
                  }
                  step={0.5}
                  value={adjustments.reflectLine ?? 60}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={20}
                  label={t('reflect.ripple')}
                  max={100}
                  min={0}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, reflectRipple: Number(e.target.value) }))
                  }
                  step={1}
                  value={adjustments.reflectRipple ?? 20}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                <Slider
                  defaultValue={30}
                  label={t('reflect.fade')}
                  max={100}
                  min={0}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, reflectFade: Number(e.target.value) }))
                  }
                  step={1}
                  value={adjustments.reflectFade ?? 30}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
              </>
            )}
          </div>

          <div className="p-2 bg-bg-tertiary rounded-md">
            <div className="mb-2">
              <Switch
                label={t('motion.title')}
                checked={!!adjustments.motionEnabled}
                onChange={(checked: boolean) =>
                  setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, motionEnabled: checked }))
                }
              />
            </div>
            {adjustments.motionEnabled && (
              <>
                <Dropdown
                  className="w-full mb-2"
                  options={[
                    { label: t('motion.movement'), value: 'movement' },
                    { label: t('motion.water'), value: 'water' },
                    { label: t('motion.trails'), value: 'trails' },
                  ]}
                  value={adjustments.motionMode || 'movement'}
                  onChange={(mode: string) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, motionMode: mode }))
                  }
                />
                <Dropdown
                  className="w-full mb-2"
                  options={[
                    {
                      label:
                        (adjustments.motionMode || 'movement') === 'movement'
                          ? t('motion.keepSharpNone')
                          : t('effectsRegion.wholePhoto'),
                      value: '',
                    },
                    ...(adjustments.masks || []).map((m: any) => ({
                      label:
                        (adjustments.motionMode || 'movement') === 'movement'
                          ? t('motion.keepSharp', { name: m.name })
                          : m.name,
                      value: m.id,
                    })),
                  ]}
                  value={adjustments.motionMaskId || ''}
                  onChange={(maskId: string) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, motionMaskId: maskId || null }))
                  }
                />
                <Slider
                  defaultValue={0}
                  label={t('motion.angle')}
                  max={90}
                  min={-90}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, motionAngle: Number(e.target.value) }))
                  }
                  step={1}
                  value={adjustments.motionAngle ?? 0}
                  onDragStateChange={onDragStateChange}
                />
                <Slider
                  defaultValue={6}
                  label={t('motion.length')}
                  max={20}
                  min={1}
                  onChange={(e: any) =>
                    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, motionLength: Number(e.target.value) }))
                  }
                  step={0.5}
                  value={adjustments.motionLength ?? 6}
                  onDragStateChange={onDragStateChange}
                  fillOrigin="min"
                />
                {adjustments.motionMode === 'trails' && (
                  <>
                    <Slider
                      defaultValue={70}
                      label={t('motion.threshold')}
                      max={100}
                      min={10}
                      onChange={(e: any) =>
                        setAdjustments((prev: Partial<Adjustments>) => ({
                          ...prev,
                          motionThreshold: Number(e.target.value),
                        }))
                      }
                      step={1}
                      value={adjustments.motionThreshold ?? 70}
                      onDragStateChange={onDragStateChange}
                      fillOrigin="min"
                    />
                    <Slider
                      defaultValue={60}
                      label={t('motion.intensity')}
                      max={100}
                      min={0}
                      onChange={(e: any) =>
                        setAdjustments((prev: Partial<Adjustments>) => ({
                          ...prev,
                          motionIntensity: Number(e.target.value),
                        }))
                      }
                      step={1}
                      value={adjustments.motionIntensity ?? 60}
                      onDragStateChange={onDragStateChange}
                      fillOrigin="min"
                    />
                  </>
                )}
                <Text variant={TextVariants.small} className="mt-1 block">
                  {t(`motion.hint.${adjustments.motionMode || 'movement'}` as any)}
                </Text>
              </>
            )}
          </div>
        </div>
      )}

      <div className="p-2 bg-bg-tertiary rounded-md">
        <Text variant={TextVariants.heading} className="mb-2">
          {t('adjustments.effects.creative')}
        </Text>

        <Slider
          label={t('adjustments.effects.glow')}
          max={100}
          min={0}
          onChange={(e: any) => handleAdjustmentChange(CreativeAdjustment.GlowAmount, e.target.value)}
          step={1}
          value={adjustments.glowAmount}
          onDragStateChange={onDragStateChange}
        />

        <Slider
          label={t('adjustments.effects.halation')}
          max={100}
          min={0}
          onChange={(e: any) => handleAdjustmentChange(CreativeAdjustment.HalationAmount, e.target.value)}
          step={1}
          value={adjustments.halationAmount}
          onDragStateChange={onDragStateChange}
        />

        {!isForMask && (
          <Slider
            label={t('adjustments.effects.lightFlares')}
            max={100}
            min={0}
            onChange={(e: any) => handleAdjustmentChange(CreativeAdjustment.FlareAmount, e.target.value)}
            step={1}
            value={adjustments.flareAmount}
            onDragStateChange={onDragStateChange}
          />
        )}
      </div>

      {!isForMask && (
        <div className="space-y-2.5">
          <div className="p-2 bg-bg-tertiary rounded-md">
            <Text variant={TextVariants.heading} className="mb-2">
              {t('adjustments.effects.lensBlur')}
            </Text>

            <Switch
              label={t('adjustments.effects.lensBlur')}
              checked={!!adjustments.lensBlurEnabled}
              onChange={handleLensBlurToggle}
            />

            <div
              className={`grid transition-all duration-300 ease-in-out ${
                adjustments.lensBlurEnabled ? 'grid-rows-[1fr] opacity-100' : 'grid-rows-[0fr] opacity-0'
              }`}
            >
              <div className="overflow-hidden">
                <div className="space-y-4 pt-4 pb-1">
                  {isGeneratingDepth ? (
                    <div className="flex flex-col items-center justify-center gap-1 p-4 text-text-secondary text-center">
                      <div className="flex items-center gap-2">
                        <Loader2 size={16} className="animate-spin shrink-0" />
                        <Text variant={TextVariants.label}>
                          {aiModelDownloadStatus
                            ? t('editor.masks.settings.aiModelDownloading')
                            : t('editor.ai.generatingDepthMap')}
                        </Text>
                      </div>
                      {aiModelDownloadStatus && (
                        <Text variant={TextVariants.small} className="text-accent">
                          {aiModelDownloadStatus}
                        </Text>
                      )}
                    </div>
                  ) : (
                    <>
                      <Slider
                        label={t('adjustments.effects.amount')}
                        max={100}
                        min={0}
                        defaultValue={40}
                        onChange={(e: any) => handleAdjustmentChange(Effect.LensBlurAmount, e.target.value)}
                        step={1}
                        value={adjustments.lensBlurAmount ?? 50}
                        onDragStateChange={onDragStateChange}
                        fillOrigin="min"
                      />

                      <Slider
                        label={t('adjustments.effects.lensDiffusion')}
                        max={100}
                        min={0}
                        defaultValue={0}
                        onChange={(e: any) => handleAdjustmentChange(Effect.lensBlurDiffusion, e.target.value)}
                        step={1}
                        value={adjustments.lensBlurDiffusion ?? 0}
                        onDragStateChange={onDragStateChange}
                      />

                      <BokehShapeSwitch
                        selectedShape={adjustments.lensBlurShape || 'circle'}
                        onShapeChange={(shapeId) =>
                          setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, [Effect.LensBlurShape]: shapeId }))
                        }
                      />

                      <DepthRangePicker
                        minDepth={100 - (adjustments.lensBlurMaxDepth ?? 100)}
                        maxDepth={100 - (adjustments.lensBlurMinDepth ?? 20)}
                        minFade={adjustments.lensBlurMaxFade ?? 20}
                        maxFade={adjustments.lensBlurMinFade ?? 20}
                        defaultMinDepth={0}
                        defaultMaxDepth={80}
                        defaultMinFade={20}
                        defaultMaxFade={20}
                        onChange={(values: {
                          minDepth: number;
                          maxDepth: number;
                          minFade: number;
                          maxFade: number;
                        }) => {
                          setAdjustments((prev: Partial<Adjustments>) => ({
                            ...prev,
                            lensBlurMinDepth: 100 - values.maxDepth,
                            lensBlurMaxDepth: 100 - values.minDepth,
                            lensBlurMinFade: values.maxFade,
                            lensBlurMaxFade: values.minFade,
                          }));
                        }}
                        onDragStateChange={onDragStateChange}
                      />
                    </>
                  )}
                </div>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
