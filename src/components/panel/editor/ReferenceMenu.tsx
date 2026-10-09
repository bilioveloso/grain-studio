import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { AnimatePresence, motion } from 'framer-motion';
import { ImagePlus, Palette, Sparkles, X } from 'lucide-react';
import clsx from 'clsx';

import Text from '../../ui/Text';
import { TextColors, TextVariants } from '../../../types/typography';
import { useEditorStore } from '../../../store/useEditorStore';
import { askClaude } from '../../../utils/askClaude';

const REFERENCE_EXTENSIONS = [
  'jpg',
  'jpeg',
  'png',
  'webp',
  'heic',
  'tif',
  'tiff',
  'dng',
  'cr2',
  'cr3',
  'nef',
  'arw',
  'raf',
];

/**
 * Match a look: pick any photo (even a screenshot) and Claude recreates its colours and light,
 * or its whole look including effects, on the open photo with real edits, then saves a version.
 */
export default function ReferenceMenu() {
  const { t } = useTranslation();
  const hasImage = useEditorStore((s) => !!s.selectedImage);
  const referencePath = useEditorStore((s) => s.referenceImagePath);
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onOpen = () => setOpen(true);
    window.addEventListener('studio-open-reference', onOpen);
    return () => window.removeEventListener('studio-open-reference', onOpen);
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

  const pick = async () => {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: 'Images', extensions: REFERENCE_EXTENSIONS }],
    });
    if (typeof picked === 'string') useEditorStore.getState().setEditor({ referenceImagePath: picked });
  };

  const ask = (promptKey: string) => {
    askClaude(t(promptKey as any));
    setOpen(false);
  };

  const fileName = referencePath?.split(/[\\/]/).pop();

  return (
    <div className="relative" ref={rootRef}>
      <button
        className={clsx(
          'bg-surface p-2 rounded-full hover:bg-card-active transition-colors',
          open || referencePath ? 'text-accent' : 'text-text-primary',
        )}
        onClick={() => setOpen((v) => !v)}
        data-tooltip={t('reference.title')}
      >
        <Palette size={20} />
      </button>

      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, y: -10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            transition={{ duration: 0.15, ease: 'easeOut' }}
            className="absolute top-full right-0 mt-3 w-72 bg-surface/95 backdrop-blur-md border border-text-secondary/10 shadow-xl rounded-lg z-50 p-2 flex flex-col gap-2"
          >
            <Text variant={TextVariants.small} color={TextColors.secondary} className="px-1">
              {t('reference.hint')}
            </Text>

            <div className="flex items-center gap-1.5">
              <button
                className="flex-1 min-w-0 flex items-center gap-2 px-2 h-8 rounded bg-bg-primary hover:bg-card-active text-xs text-text-primary"
                onClick={pick}
              >
                <ImagePlus size={13} className="shrink-0 text-accent" />
                <span className="truncate">{fileName || t('reference.pick')}</span>
              </button>
              {referencePath && (
                <button
                  className="p-2 rounded text-text-secondary hover:text-text-primary hover:bg-card-active"
                  onClick={() => useEditorStore.getState().setEditor({ referenceImagePath: null })}
                  data-tooltip={t('reference.clear')}
                >
                  <X size={13} />
                </button>
              )}
            </div>

            <button
              className="flex items-center gap-2 px-2 h-8 rounded text-xs bg-accent text-button-text disabled:opacity-40"
              disabled={!referencePath}
              onClick={() => ask('reference.promptColor')}
            >
              <Palette size={13} />
              {t('reference.colorLight')}
            </button>
            <button
              className="flex items-center gap-2 px-2 h-8 rounded text-xs bg-bg-primary text-text-primary hover:bg-card-active disabled:opacity-40"
              disabled={!referencePath}
              onClick={() => ask('reference.promptLook')}
            >
              <Sparkles size={13} className="text-accent" />
              {t('reference.fullLook')}
            </button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
