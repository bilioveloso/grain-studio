import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AnimatePresence, motion } from 'framer-motion';
import { History } from 'lucide-react';
import clsx from 'clsx';

import Text from '../../ui/Text';
import { TextColors, TextVariants } from '../../../types/typography';
import { useEditorStore } from '../../../store/useEditorStore';
import { Adjustments } from '../../../utils/adjustments';

const humanize = (key: string) => key.replace(/([a-z0-9])([A-Z])/g, '$1 $2').replace(/^./, (c) => c.toUpperCase());

const describeValue = (value: unknown) => {
  if (typeof value === 'number') {
    const rounded = +value.toFixed(2);
    return rounded > 0 ? `+${rounded}` : `${rounded}`;
  }
  if (typeof value === 'boolean') return value ? 'on' : 'off';
  return '';
};

/** What changed between two history steps, Lightroom style: "Exposure +0.5". */
export function describeStep(prev: Adjustments, next: Adjustments): string {
  const changed = (Object.keys(next) as Array<keyof Adjustments>).filter(
    (key) => JSON.stringify(prev[key]) !== JSON.stringify(next[key]),
  );
  if (changed.length === 0) return '';
  const first = changed[0];
  const label = `${humanize(String(first))} ${describeValue(next[first])}`.trim();
  return changed.length > 1 ? `${label} (+${changed.length - 1})` : label;
}

/**
 * Lightroom's History panel, in the toolbar next to undo/redo: every step of this session, newest first.
 * Clicking one goes back (or forward) to it; steps after the current one are the ones redo would bring back.
 */
export default function HistoryMenu() {
  const { t } = useTranslation();
  const history = useEditorStore((s) => s.history);
  const historyIndex = useEditorStore((s) => s.historyIndex);
  const goToHistoryIndex = useEditorStore((s) => s.goToHistoryIndex);
  const hasImage = useEditorStore((s) => !!s.selectedImage);
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const closeOutside = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('mousedown', closeOutside);
    return () => window.removeEventListener('mousedown', closeOutside);
  }, [open]);

  const steps = useMemo(
    () =>
      history
        .map((state, index) => ({
          index,
          label:
            index === 0 ? t('historyPanel.opened') : describeStep(history[index - 1], state) || t('historyPanel.edit'),
        }))
        .reverse(),
    [history, t],
  );

  if (!hasImage) return null;

  return (
    <div className="relative" ref={rootRef}>
      <button
        className={clsx(
          'relative bg-surface p-2 rounded-full hover:bg-card-active transition-colors',
          open ? 'text-accent' : 'text-text-primary',
        )}
        onClick={() => setOpen((v) => !v)}
        data-tooltip={t('historyPanel.title')}
      >
        <History size={20} />
      </button>

      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, y: -10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            transition={{ duration: 0.15, ease: 'easeOut' }}
            className="absolute top-full right-0 mt-3 w-64 bg-surface/95 backdrop-blur-md border border-text-secondary/10 shadow-xl rounded-lg z-50 p-2 flex flex-col gap-1"
          >
            <Text variant={TextVariants.small} color={TextColors.secondary} className="px-1">
              {t('historyPanel.title')}
            </Text>
            <div className="flex flex-col gap-0.5 max-h-80 overflow-y-auto custom-scrollbar">
              {steps.map((step) => (
                <button
                  key={step.index}
                  className={clsx(
                    'text-left px-2 py-1 rounded text-xs truncate',
                    step.index === historyIndex
                      ? 'bg-accent/20 text-accent'
                      : step.index > historyIndex
                        ? 'text-text-secondary/60 hover:bg-bg-primary'
                        : 'text-text-primary hover:bg-bg-primary',
                  )}
                  onClick={() => goToHistoryIndex(step.index)}
                >
                  {step.label}
                </button>
              ))}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
