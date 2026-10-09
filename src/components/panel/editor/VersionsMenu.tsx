import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';
import { AnimatePresence, motion } from 'framer-motion';
import { Clock, Eye, GalleryVerticalEnd, ImageIcon, Pencil, Save, Smartphone, Trash2 } from 'lucide-react';
import clsx from 'clsx';

import { Invokes } from '../../ui/AppProperties';
import Text from '../../ui/Text';
import { TextColors, TextVariants } from '../../../types/typography';
import { useEditorStore } from '../../../store/useEditorStore';
import { INITIAL_ADJUSTMENTS } from '../../../utils/adjustments';

interface VersionSummary {
  id: string;
  name: string;
  createdAt: number;
  auto: boolean;
  original: boolean;
  /** The edit made in the iPhone's Photos app, read from the .AAE / IMG_E files beside the photo */
  iphone?: boolean;
  /** Apple's own render of that edit */
  render?: string;
  /** False when only Apple's render exists, with no recipe to put on the sliders */
  editable: boolean;
}

const openRender = (render: string) => window.dispatchEvent(new CustomEvent('studio-open-image', { detail: render }));

/**
 * Versions of the open photo, in the toolbar next to undo/redo so you can flip between looks.
 * Clicking a version shows it; "Current edit" brings back what you had before you started
 * flipping. Every switch is a normal edit, so Ctrl+Z still works.
 */
export default function VersionsMenu() {
  const { t } = useTranslation();
  const path = useEditorStore((s) => s.selectedImage?.path);
  const [open, setOpen] = useState(false);
  const [versions, setVersions] = useState<VersionSummary[]>([]);
  const [name, setName] = useState('');
  const [shownId, setShownId] = useState<string | null>(null);
  // The edit you had before browsing, so comparing never loses it.
  const workingEdit = useRef<any>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    if (!path) return setVersions([]);
    try {
      setVersions(await invoke<VersionSummary[]>(Invokes.ListVersions, { path }));
    } catch (err) {
      console.error('Could not list versions', err);
    }
  }, [path]);

  // A different photo starts a fresh browse.
  useEffect(() => {
    workingEdit.current = null;
    setShownId(null);
    refresh();
  }, [refresh]);

  useEffect(() => {
    const onChanged = () => refresh();
    window.addEventListener('studio-versions-changed', onChanged);
    return () => window.removeEventListener('studio-versions-changed', onChanged);
  }, [refresh]);

  useEffect(() => {
    if (!open) return;
    refresh();
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [open, refresh]);

  // Versions are complete edits laid over the defaults; the Original has none, so it is
  // exactly the defaults - the photo as shot. Keys a version lacks never leak from the
  // edit that happened to be on screen.
  const apply = (adjustments: any) => {
    const store = useEditorStore.getState();
    const next = { ...structuredClone(INITIAL_ADJUSTMENTS), ...(adjustments || {}) };
    store.setEditor({ adjustments: next });
    store.pushHistory(next);
  };

  const show = async (version: VersionSummary) => {
    if (!path) return;
    try {
      if (workingEdit.current === null) workingEdit.current = structuredClone(useEditorStore.getState().adjustments);
      apply(await invoke<any>(Invokes.RestoreVersion, { path, versionId: version.id }));
      setShownId(version.id);
    } catch (err) {
      toast.error(String(err));
    }
  };

  const backToCurrent = () => {
    if (workingEdit.current === null) return;
    apply(workingEdit.current);
    workingEdit.current = null;
    setShownId(null);
  };

  const save = async () => {
    if (!path) return;
    try {
      const { adjustments } = useEditorStore.getState();
      setVersions(await invoke<VersionSummary[]>(Invokes.SaveVersion, { path, name: name.trim(), adjustments }));
      setName('');
      // What is on screen is now saved, so there is nothing separate to return to.
      workingEdit.current = null;
      setShownId(null);
    } catch (err) {
      toast.error(String(err));
    }
  };

  const remove = async (version: VersionSummary) => {
    if (!path) return;
    try {
      setVersions(await invoke<VersionSummary[]>(Invokes.DeleteVersion, { path, versionId: version.id }));
      if (shownId === version.id) setShownId(null);
    } catch (err) {
      toast.error(String(err));
    }
  };

  const when = (seconds: number) =>
    new Date(seconds * 1000).toLocaleString(undefined, {
      day: 'numeric',
      month: 'short',
      hour: '2-digit',
      minute: '2-digit',
    });

  if (!path) return null;

  return (
    <div className="relative" ref={rootRef}>
      <button
        className={clsx(
          'relative bg-surface p-2 rounded-full hover:bg-card-active transition-colors',
          open || shownId ? 'text-accent' : 'text-text-primary',
        )}
        onClick={() => setOpen((v) => !v)}
        data-tooltip={t('versions.title')}
      >
        <GalleryVerticalEnd size={20} />
        {versions.length > 0 && (
          <span className="absolute -top-0.5 -right-0.5 min-w-4 h-4 px-1 rounded-full bg-accent text-button-text text-[10px] leading-4 text-center">
            {versions.length}
          </span>
        )}
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
            <div className="flex gap-1.5">
              <input
                className="flex-1 min-w-0 rounded border border-border-color bg-bg-primary px-2 h-7 text-xs text-text-primary focus:border-accent focus:outline-none"
                onChange={(e) => setName(e.target.value)}
                onKeyDown={(e) => {
                  e.stopPropagation();
                  if (e.key === 'Enter') save();
                }}
                placeholder={t('versions.namePlaceholder')}
                value={name}
              />
              <button
                className="px-2 h-7 rounded bg-accent text-button-text text-xs flex items-center gap-1"
                onClick={save}
                data-tooltip={t('versions.saveTooltip')}
              >
                <Save size={12} />
                {t('versions.save')}
              </button>
            </div>

            {shownId && (
              <button
                className="flex items-center gap-2 px-2 py-1.5 rounded text-left text-xs text-text-primary bg-card-active hover:bg-bg-primary"
                onClick={backToCurrent}
              >
                <Pencil size={12} className="text-accent" />
                {t('versions.backToCurrent')}
              </button>
            )}

            {versions.length === 0 ? (
              <Text variant={TextVariants.small} color={TextColors.secondary} className="px-1 pb-1">
                {t('versions.empty')}
              </Text>
            ) : (
              <div className="flex flex-col gap-0.5 max-h-72 overflow-y-auto custom-scrollbar">
                {versions.map((version) => (
                  <div
                    key={version.id}
                    className={clsx(
                      'group flex items-center gap-2 px-2 py-1 rounded cursor-pointer',
                      shownId === version.id ? 'bg-accent/20' : 'hover:bg-bg-primary',
                    )}
                    onClick={() => (version.editable ? show(version) : version.render && openRender(version.render))}
                    data-tooltip={t(version.iphone ? 'versions.iphoneTooltip' : 'versions.showTooltip')}
                  >
                    {version.iphone ? (
                      <Smartphone size={11} className="text-text-secondary shrink-0" />
                    ) : version.original ? (
                      <ImageIcon size={11} className="text-text-secondary shrink-0" />
                    ) : (
                      version.auto && <Clock size={11} className="text-text-secondary shrink-0" />
                    )}
                    <div className="min-w-0 flex-1">
                      <Text
                        variant={TextVariants.small}
                        className={clsx('truncate block', shownId === version.id && 'text-accent')}
                      >
                        {version.iphone
                          ? t('versions.iphone')
                          : version.original
                            ? t('versions.original')
                            : version.name || t('versions.autoLabel')}
                      </Text>
                      <Text variant={TextVariants.small} color={TextColors.secondary} className="truncate block">
                        {when(version.createdAt)}
                      </Text>
                    </div>
                    {version.render && (
                      <button
                        className="p-1 rounded text-text-secondary hover:text-accent"
                        onClick={(e) => {
                          e.stopPropagation();
                          openRender(version.render!);
                        }}
                        data-tooltip={t('versions.openRender')}
                      >
                        <Eye size={13} />
                      </button>
                    )}
                    {!version.original && !version.iphone && (
                      <button
                        className="opacity-0 group-hover:opacity-100 p-1 rounded text-text-secondary hover:text-red-400"
                        onClick={(e) => {
                          e.stopPropagation();
                          remove(version);
                        }}
                        data-tooltip={t('versions.delete')}
                      >
                        <Trash2 size={13} />
                      </button>
                    )}
                  </div>
                ))}
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
