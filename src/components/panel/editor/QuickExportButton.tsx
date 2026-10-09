import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2, Share } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';

import Text from '../../ui/Text';
import { TextColors, TextVariants } from '../../../types/typography';
import { useQuickExport } from '../../../hooks/useQuickExport';

/** Toolbar button: pick a preset, the open photo exports to Studio\03_finished immediately. */
export default function QuickExportButton() {
  const { t } = useTranslation();
  const { presets, exportWith, isExporting } = useQuickExport();
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', close);
    return () => document.removeEventListener('mousedown', close);
  }, [open]);

  return (
    <div className="relative" ref={rootRef}>
      <button
        className="bg-surface text-text-primary p-2 rounded-full hover:bg-card-active transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
        disabled={isExporting}
        onClick={() => setOpen((v) => !v)}
        data-tooltip={isExporting ? t('quickExport.exporting') : t('quickExport.tooltip')}
        aria-expanded={open}
      >
        {isExporting ? <Loader2 size={20} className="animate-spin" /> : <Share size={20} />}
      </button>

      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, y: -10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            transition={{ duration: 0.15, ease: 'easeOut' }}
            className="absolute top-full right-0 mt-3 w-64 bg-surface/90 backdrop-blur-md border border-text-secondary/10 shadow-xl rounded-lg z-50 flex flex-col py-1.5 px-0.5"
          >
            <Text variant={TextVariants.small} color={TextColors.secondary} className="px-3 py-1">
              {t('quickExport.destination')}
            </Text>
            {presets.length === 0 && (
              <Text variant={TextVariants.small} color={TextColors.secondary} className="px-3 py-2">
                {t('quickExport.noPresets')}
              </Text>
            )}
            {presets.map((preset) => (
              <button
                key={preset.id}
                className="text-left px-3 py-2 mx-1 my-0.5 rounded-md hover:bg-bg-primary transition-colors"
                onClick={() => {
                  setOpen(false);
                  exportWith(preset);
                }}
              >
                <Text as="span" className="truncate">
                  {preset.name}
                </Text>
              </button>
            ))}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
