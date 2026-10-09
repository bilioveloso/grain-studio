import { useRef } from 'react';
import { useTranslation } from 'react-i18next';
import clsx from 'clsx';

import { Adjustments } from '../../../utils/adjustments';
import { useEditorStore } from '../../../store/useEditorStore';

interface EffectGuidesProps {
  adjustments: Adjustments;
  setAdjustments(fn: (prev: Adjustments) => Adjustments): void;
  imageRenderSize: { width: number; height: number; offsetX: number; offsetY: number };
  isCropping: boolean;
}

interface Guide {
  key: string;
  label: string;
  orientation: 'horizontal' | 'vertical';
  value: number;
  min: number;
  max: number;
}

/**
 * Draggable guides on the photo for placing creative effects: the reflection waterline,
 * the pixel-stretch line and band, and the fractal-glass box. Values are percentages of the
 * photo, the same numbers the sliders show.
 */
export default function EffectGuides({ adjustments, setAdjustments, imageRenderSize, isCropping }: EffectGuidesProps) {
  const { t } = useTranslation();
  const effectGuide = useEditorStore((s) => s.effectGuide);
  const boxRef = useRef<HTMLDivElement>(null);
  const frame = useRef<number | null>(null);
  const adj: any = adjustments;

  if (!effectGuide || isCropping || !imageRenderSize.width || !imageRenderSize.height) return null;

  const num = (key: string, fallback: number) => (typeof adj[key] === 'number' ? adj[key] : fallback);
  const guides: Guide[] = [];

  if (effectGuide === 'reflect') {
    guides.push({
      key: 'reflectLine',
      label: t('guides.waterline'),
      orientation: 'horizontal',
      value: num('reflectLine', 60),
      min: 5,
      max: 95,
    });
  } else if (effectGuide === 'stretch') {
    const vertical = adj.stretchAxis === 'vertical';
    const bandStart = num('stretchBandStart', 30);
    const bandEnd = num('stretchBandEnd', 70);
    // Sideways streaks start at a vertical line; the band is a horizontal strip (and vice versa).
    guides.push({
      key: 'stretchPosition',
      label: t('guides.stretchLine'),
      orientation: vertical ? 'horizontal' : 'vertical',
      value: num('stretchPosition', 50),
      min: 0,
      max: 100,
    });
    guides.push({
      key: 'stretchBandStart',
      label: t('guides.bandStart'),
      orientation: vertical ? 'vertical' : 'horizontal',
      value: bandStart,
      min: 0,
      max: bandEnd - 1,
    });
    guides.push({
      key: 'stretchBandEnd',
      label: t('guides.bandEnd'),
      orientation: vertical ? 'vertical' : 'horizontal',
      value: bandEnd,
      min: bandStart + 1,
      max: 100,
    });
  } else if (effectGuide === 'glass') {
    const top = num('glassStart', 0);
    const bottom = num('glassBottom', 100);
    const left = num('glassLeft', 0);
    const right = num('glassRight', 100);
    guides.push({
      key: 'glassStart',
      label: t('guides.top'),
      orientation: 'horizontal',
      value: top,
      min: 0,
      max: bottom - 2,
    });
    guides.push({
      key: 'glassBottom',
      label: t('guides.bottom'),
      orientation: 'horizontal',
      value: bottom,
      min: top + 2,
      max: 100,
    });
    guides.push({
      key: 'glassLeft',
      label: t('guides.left'),
      orientation: 'vertical',
      value: left,
      min: 0,
      max: right - 2,
    });
    guides.push({
      key: 'glassRight',
      label: t('guides.right'),
      orientation: 'vertical',
      value: right,
      min: left + 2,
      max: 100,
    });
  }

  const startDrag = (guide: Guide) => (e: React.PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    const target = e.currentTarget;
    target.setPointerCapture(e.pointerId);
    useEditorStore.getState().setEditor({ isSliderDragging: true });

    const move = (ev: PointerEvent) => {
      const rect = boxRef.current?.getBoundingClientRect();
      if (!rect) return;
      const raw =
        guide.orientation === 'horizontal'
          ? ((ev.clientY - rect.top) / rect.height) * 100
          : ((ev.clientX - rect.left) / rect.width) * 100;
      const value = Math.round(Math.min(guide.max, Math.max(guide.min, raw)) * 2) / 2;
      // One update per frame: pointer events arrive far faster than the preview renders.
      if (frame.current !== null) cancelAnimationFrame(frame.current);
      frame.current = requestAnimationFrame(() => {
        frame.current = null;
        setAdjustments((prev: any) => ({ ...prev, [guide.key]: value }));
      });
    };
    const up = (ev: PointerEvent) => {
      target.releasePointerCapture(ev.pointerId);
      target.removeEventListener('pointermove', move);
      target.removeEventListener('pointerup', up);
      useEditorStore.getState().setEditor({ isSliderDragging: false });
    };
    target.addEventListener('pointermove', move);
    target.addEventListener('pointerup', up);
  };

  return (
    <div
      ref={boxRef}
      className="absolute pointer-events-none"
      style={{
        left: `${imageRenderSize.offsetX}px`,
        top: `${imageRenderSize.offsetY}px`,
        width: `${imageRenderSize.width}px`,
        height: `${imageRenderSize.height}px`,
        zIndex: 6,
      }}
    >
      {guides.map((guide) => {
        const horizontal = guide.orientation === 'horizontal';
        return (
          <div
            key={guide.key}
            data-effect-guide
            className={clsx(
              'absolute pointer-events-auto group flex items-center justify-center',
              horizontal
                ? 'left-0 right-0 h-4 -translate-y-1/2 cursor-row-resize'
                : 'top-0 bottom-0 w-4 -translate-x-1/2 cursor-col-resize',
            )}
            style={horizontal ? { top: `${guide.value}%` } : { left: `${guide.value}%` }}
            onPointerDown={startDrag(guide)}
          >
            <div
              className={clsx(
                'bg-accent shadow-[0_0_0_1px_rgba(0,0,0,0.5)] group-hover:opacity-100 opacity-80',
                horizontal ? 'h-px w-full' : 'w-px h-full',
              )}
            />
            <span
              className={clsx(
                'absolute px-1.5 py-0.5 rounded bg-black/70 text-white text-[10px] whitespace-nowrap pointer-events-none',
                horizontal ? 'left-2' : 'top-2',
              )}
            >
              {guide.label} {guide.value}%
            </span>
          </div>
        );
      })}
    </div>
  );
}
