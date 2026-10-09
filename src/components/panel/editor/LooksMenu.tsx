import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AnimatePresence, motion } from 'framer-motion';
import { WandSparkles } from 'lucide-react';
import clsx from 'clsx';

import Text from '../../ui/Text';
import { TextColors, TextVariants } from '../../../types/typography';
import { useEditorStore } from '../../../store/useEditorStore';
import { CLAUDE_LOOKS } from '../../../utils/claudeLooks';
import { askClaude } from '../../../utils/askClaude';

/**
 * One-click viral looks: each card hands Claude a prepared request; Claude builds the look with
 * masks and effects on the open photo and saves it as a version named after the look.
 */
export default function LooksMenu() {
  const { t } = useTranslation();
  const hasImage = useEditorStore((s) => !!s.selectedImage);
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onOpen = () => setOpen(true);
    window.addEventListener('studio-open-looks', onOpen);
    return () => window.removeEventListener('studio-open-looks', onOpen);
  }, []);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [open]);

  if (!hasImage) return null;

  return (
    <div className="relative" ref={rootRef}>
      <button
        className={clsx(
          'bg-surface p-2 rounded-full hover:bg-card-active transition-colors',
          open ? 'text-accent' : 'text-text-primary',
        )}
        onClick={() => setOpen((v) => !v)}
        data-tooltip={t('looks.title')}
      >
        <WandSparkles size={20} />
      </button>

      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, y: -10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            transition={{ duration: 0.15, ease: 'easeOut' }}
            className="absolute top-full right-0 mt-3 w-[26rem] bg-surface/95 backdrop-blur-md border border-text-secondary/10 shadow-xl rounded-lg z-50 p-2 flex flex-col gap-2"
          >
            <Text variant={TextVariants.small} color={TextColors.secondary} className="px-1">
              {t('looks.hint')}
            </Text>
            <div className="grid grid-cols-2 gap-1.5">
              {CLAUDE_LOOKS.map((look) => (
                <button
                  key={look.id}
                  className="flex items-start gap-2 p-2 rounded-md bg-bg-primary hover:bg-card-active text-left transition-colors"
                  onClick={() => {
                    askClaude(look.prompt);
                    setOpen(false);
                  }}
                >
                  <look.icon size={15} className="text-accent shrink-0 mt-0.5" />
                  <span className="min-w-0">
                    <span className="block text-xs text-text-primary">{t(`looks.${look.id}.label` as any)}</span>
                    <span className="block text-[11px] leading-snug text-text-secondary">
                      {t(`looks.${look.id}.description` as any)}
                    </span>
                  </span>
                </button>
              ))}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
