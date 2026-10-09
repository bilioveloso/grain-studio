import { useEffect, useRef, useState } from 'react';
import { Check, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { RenderSize } from '../../../hooks/useImageRenderSize';
import { useEditorStore } from '../../../store/useEditorStore';
import { Annotation, Point, clamp01, newPin, newRing } from '../../../utils/annotations';

/**
 * Marks the user puts on the photo for the assistant: click a spot, or circle something, and say what should
 * change there. Sits over the photo inside the zoom transform, so a mark stays on its spot while you zoom and
 * pan; the pins and the ring's stroke are scaled back so they stay the same size on screen.
 */
export default function AnnotationLayer({ renderSize, zoom }: { renderSize: RenderSize; zoom: number }) {
  const { t } = useTranslation();
  const annotateMode = useEditorStore((s) => s.annotateMode);
  const annotateTool = useEditorStore((s) => s.annotateTool);
  const annotations = useEditorStore((s) => s.annotations);
  const setEditor = useEditorStore((s) => s.setEditor);
  const selectedPath = useEditorStore((s) => s.selectedImage?.path);
  const editorTool = useEditorStore((s) => s.editorTool);
  const activeMaskId = useEditorStore((s) => s.activeMaskId);
  const isWbPickerActive = useEditorStore((s) => s.isWbPickerActive);
  const isStraightenActive = useEditorStore((s) => s.isStraightenActive);
  const boxRef = useRef<HTMLDivElement>(null);
  // The path lives in a ref, not in state: pointermove fires faster than React commits, and reading the drawn
  // points back out of state dropped every move that landed in the same frame — a full ring arrived as a nonagon.
  const pathRef = useRef<Point[]>([]);
  const [drawing, setDrawing] = useState<Point[] | null>(null);
  // A mark put in roughly the right place should be nudgeable without being deleted and drawn again. Dragging
  // the numbered badge moves the mark; for a circle it carries the whole drawn shape with it, because moving
  // the label away from the ring it belongs to would be nonsense.
  const dragRef = useRef<{ id: string; from: Point; mark: Annotation } | null>(null);

  // Marking is a mode, and this layer takes the pointer for as long as it is on. Anything else that wants the
  // pointer -- picking up the zoom tool, editing a mask, the white balance dropper, the straighten line -- ends
  // it, and so does opening another photo. Without this the marking layer quietly swallows every drag and the
  // rest of the editor feels broken.
  useEffect(() => {
    if (annotateMode) setEditor({ annotateMode: false });
    // Deliberately not watching annotateMode: this fires when something else takes over, not when it changes.
  }, [selectedPath, editorTool, activeMaskId, isWbPickerActive, isStraightenActive]);

  if (!annotateMode && annotations.length === 0) return null;
  if (!renderSize.width || !renderSize.height) return null;

  const startDrag = (e: React.PointerEvent, mark: Annotation) => {
    const p = at(e);
    if (!p || e.button !== 0) return;
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
    dragRef.current = { id: mark.id, from: p, mark };
  };

  const moveDrag = (e: React.PointerEvent) => {
    const drag = dragRef.current;
    if (!drag) return;
    const p = at(e);
    if (!p) return;
    e.stopPropagation();
    const dx = p.x - drag.from.x;
    const dy = p.y - drag.from.y;
    setEditor((state) => ({
      annotations: state.annotations.map((a: Annotation) =>
        a.id !== drag.id
          ? a
          : {
              ...a,
              x: clamp01(drag.mark.x + dx),
              y: clamp01(drag.mark.y + dy),
              points: drag.mark.points.map((q) => ({ x: clamp01(q.x + dx), y: clamp01(q.y + dy) })),
            },
      ),
    }));
  };

  const endDrag = (e: React.PointerEvent) => {
    if (!dragRef.current) return;
    e.stopPropagation();
    dragRef.current = null;
  };

  const update = (id: string, note: string) =>
    setEditor((state) => ({
      annotations: state.annotations.map((a: Annotation) => (a.id === id ? { ...a, note } : a)),
    }));
  const remove = (id: string) =>
    setEditor((state) => ({ annotations: state.annotations.filter((a: Annotation) => a.id !== id) }));

  const at = (e: React.MouseEvent | React.PointerEvent): Point | null => {
    const box = boxRef.current?.getBoundingClientRect();
    if (!box) return null;
    return { x: (e.clientX - box.left) / box.width, y: (e.clientY - box.top) / box.height };
  };

  const addPin = (e: React.MouseEvent) => {
    const p = at(e);
    if (!p) return;
    e.stopPropagation();
    setEditor((state) => ({ annotations: [...state.annotations, newPin(p.x, p.y)] }));
  };

  const startRing = (e: React.PointerEvent) => {
    const p = at(e);
    if (!p || e.button !== 0) return;
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
    pathRef.current = [p];
    setDrawing(pathRef.current);
  };

  const extendRing = (e: React.PointerEvent) => {
    if (!pathRef.current.length) return;
    const p = at(e);
    if (!p) return;
    // Skip points the drawing hand did not really move, so a ring stays a few hundred points, not thousands.
    const last = pathRef.current[pathRef.current.length - 1];
    if (Math.abs(p.x - last.x) < 0.0015 && Math.abs(p.y - last.y) < 0.0015) return;
    pathRef.current = [...pathRef.current, p];
    setDrawing(pathRef.current);
  };

  const finishRing = (e: React.PointerEvent) => {
    if (!pathRef.current.length) return;
    e.stopPropagation();
    const path = pathRef.current;
    pathRef.current = [];
    setDrawing(null);
    if (path.length < 3) return;
    setEditor((state) => ({ annotations: [...state.annotations, newRing(path)] }));
  };

  const ringMode = annotateMode && annotateTool === 'ring';
  // Undo the canvas zoom so pins and their notes keep their size on screen.
  const unzoom = `scale(${1 / Math.max(zoom, 0.01)})`;
  const path = (points: Point[]) =>
    points.map((p, i) => `${i ? 'L' : 'M'} ${p.x * 100} ${p.y * 100}`).join(' ') + (points.length > 2 ? ' Z' : '');

  return (
    <div
      ref={boxRef}
      data-assistant-overlay
      className="absolute"
      style={{
        left: renderSize.offsetX,
        top: renderSize.offsetY,
        width: renderSize.width,
        height: renderSize.height,
        zIndex: 6,
        cursor: annotateMode ? 'crosshair' : 'default',
        pointerEvents: annotateMode ? 'auto' : 'none',
        touchAction: ringMode ? 'none' : undefined,
      }}
      onClick={annotateMode && !ringMode ? addPin : undefined}
      onPointerDown={ringMode ? startRing : undefined}
      onPointerMove={ringMode ? extendRing : undefined}
      onPointerUp={ringMode ? finishRing : undefined}
      onPointerCancel={ringMode ? finishRing : undefined}
    >
      <svg viewBox="0 0 100 100" preserveAspectRatio="none" className="absolute inset-0 w-full h-full pointer-events-none">
        {annotations
          .filter((a) => a.kind === 'ring')
          .map((a) => (
            <path
              key={a.id}
              d={path(a.points)}
              fill="rgba(248,113,113,0.12)"
              stroke="#f87171"
              strokeWidth={2}
              strokeLinejoin="round"
              vectorEffect="non-scaling-stroke"
            />
          ))}
        {drawing && drawing.length > 1 && (
          <path
            d={path(drawing)}
            fill="none"
            stroke="#f87171"
            strokeWidth={2}
            strokeLinecap="round"
            strokeLinejoin="round"
            vectorEffect="non-scaling-stroke"
          />
        )}
      </svg>

      {annotations.map((a, i) => (
        <div
          key={a.id}
          className="absolute flex items-center gap-2"
          style={{
            left: `${a.x * 100}%`,
            top: `${a.y * 100}%`,
            transform: `translate(-16px, ${a.kind === 'ring' ? '-110%' : '-50%'}) ${unzoom}`,
            transformOrigin: 'left center',
          }}
          onClick={(e) => e.stopPropagation()}
          onPointerDown={(e) => e.stopPropagation()}
        >
          <span
            className="shrink-0 w-8 h-8 rounded-full bg-white text-black text-sm font-semibold flex items-center justify-center shadow-lg ring-2 ring-black/20 pointer-events-auto cursor-grab active:cursor-grabbing touch-none select-none"
            onPointerDown={(e) => startDrag(e, a)}
            onPointerMove={moveDrag}
            onPointerUp={endDrag}
            onPointerCancel={endDrag}
            data-tooltip={t('claude.annotate.dragToMove')}
          >
            {i + 1}
          </span>
          <div className="flex items-center gap-1 rounded-full bg-black/80 backdrop-blur-sm pl-3 pr-1.5 py-1 shadow-lg pointer-events-auto">
            <input
              autoFocus={!a.note}
              className="bg-transparent text-white text-sm placeholder-white/50 focus:outline-none w-52"
              onChange={(e) => update(a.id, e.target.value)}
              onKeyDown={(e) => {
                e.stopPropagation();
                if (e.key === 'Enter' || e.key === 'Escape') (e.target as HTMLInputElement).blur();
              }}
              placeholder={t('claude.annotate.notePlaceholder')}
              value={a.note}
            />
            <button
              className="p-1 rounded-full text-white/70 hover:text-white"
              onClick={() => (document.activeElement as HTMLElement)?.blur()}
              data-tooltip={t('claude.annotate.keep')}
            >
              <Check size={14} />
            </button>
            <button
              className="p-1 rounded-full text-white/70 hover:text-white"
              onClick={() => remove(a.id)}
              data-tooltip={t('claude.annotate.remove')}
            >
              <X size={14} />
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}
