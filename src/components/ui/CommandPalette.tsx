import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Search } from 'lucide-react';
import clsx from 'clsx';

import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { toast } from 'react-toastify';

import { Invokes, Panel } from './AppProperties';
import Text from './Text';
import { TextColors, TextVariants } from '../../types/typography';
import { useEditorStore } from '../../store/useEditorStore';
import { useUIStore } from '../../store/useUIStore';
import { useEditorActions } from '../../hooks/useEditorActions';
import { CLAUDE_LOOKS } from '../../utils/claudeLooks';
import { toggleCompareView } from '../../utils/compareView';
import { askClaude } from '../../utils/askClaude';
import { useSettingsStore } from '../../store/useSettingsStore';

interface CommandPaletteProps {
  onOpenFolder(): void;
  onLibrary(): void;
  onHome(): void;
}

interface Command {
  id: string;
  label: string;
  hint?: string;
  run(): void;
  enabled?: boolean;
}

/**
 * Ctrl+K command palette: type what you want instead of hunting for the button.
 * ponytail: substring match over a small fixed list, no fuzzy-search dependency.
 */
export default function CommandPalette({ onOpenFolder, onLibrary, onHome }: CommandPaletteProps) {
  const { t } = useTranslation();
  const editorActions = useEditorActions();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [cursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.code === 'KeyK') {
        e.preventDefault();
        setQuery('');
        setCursor(0);
        setOpen((v) => !v);
      } else if (e.code === 'Escape') {
        setOpen(false);
      }
    };
    const onAsk = () => {
      setQuery('');
      setCursor(0);
      setOpen(true);
    };
    window.addEventListener('keydown', onKey, true);
    window.addEventListener('studio-open-commands', onAsk);
    return () => {
      window.removeEventListener('keydown', onKey, true);
      window.removeEventListener('studio-open-commands', onAsk);
    };
  }, []);

  useEffect(() => {
    if (open) setTimeout(() => inputRef.current?.focus(), 0);
  }, [open]);

  const commands: Command[] = useMemo(() => {
    const ui = useUIStore.getState();
    const editor = useEditorStore.getState();
    const hasImage = !!editor.selectedImage;

    return [
      { id: 'home', label: t('commands.home'), run: onHome },
      { id: 'library', label: t('commands.library'), hint: 'G', run: onLibrary },
      {
        id: 'develop',
        label: t('commands.develop'),
        hint: 'D',
        enabled: hasImage,
        run: () => ui.setUI({ activeView: 'editor' }),
      },
      { id: 'addFolder', label: t('commands.addFolder'), run: onOpenFolder },
      {
        id: 'export',
        label: t('commands.export'),
        enabled: hasImage,
        run: () => ui.setPanel(Panel.Export),
      },
      { id: 'masks', label: t('commands.masks'), enabled: hasImage, run: () => ui.setPanel(Panel.Masks) },
      { id: 'crop', label: t('commands.crop'), enabled: hasImage, run: () => ui.setPanel(Panel.Crop) },
      { id: 'presets', label: t('commands.presets'), enabled: hasImage, run: () => ui.setPanel(Panel.Presets) },
      { id: 'settings', label: t('commands.settings'), run: () => ui.setUI({ isSettingsOpen: true }) },
      { id: 'fullscreen', label: t('commands.fullscreen'), hint: 'F', run: () => ui.toggleFullScreen?.() },
      {
        id: 'autoAdjust',
        label: t('commands.autoAdjust'),
        enabled: hasImage,
        run: () => editorActions.handleAutoAdjustments(),
      },
      {
        id: 'resetAdjustments',
        label: t('commands.resetAdjustments'),
        enabled: hasImage,
        run: () => editorActions.handleResetAdjustments(),
      },
      {
        id: 'copyAdjustments',
        label: t('commands.copyAdjustments'),
        enabled: hasImage,
        run: () => editorActions.handleCopyAdjustments(),
      },
      {
        id: 'pasteAdjustments',
        label: t('commands.pasteAdjustments'),
        enabled: hasImage,
        run: () => editorActions.handlePasteAdjustments(),
      },
      {
        id: 'beforeAfter',
        label: t('commands.beforeAfter'),
        enabled: hasImage,
        run: () => editorActions.toggleShowOriginal(),
      },
      {
        id: 'compareView',
        label: t('commands.compareView'),
        enabled: hasImage,
        run: () => void toggleCompareView(),
      },
      {
        id: 'saveVersion',
        label: t('commands.saveVersion'),
        enabled: hasImage,
        run: () => {
          const { selectedImage, adjustments } = useEditorStore.getState();
          if (selectedImage?.path) {
            invoke(Invokes.SaveVersion, { path: selectedImage.path, name: '', adjustments })
              .then(() => window.dispatchEvent(new CustomEvent('studio-versions-changed')))
              .catch((err) => console.error('Could not save version', err));
          }
        },
      },
      ...CLAUDE_LOOKS.map((look) => ({
        id: `look-${look.id}`,
        label: t('commands.look', { name: t(`looks.${look.id}.label` as any) }),
        enabled: hasImage,
        run: () => askClaude(look.prompt),
      })),
      {
        id: 'matchLook',
        label: t('commands.matchLook'),
        enabled: hasImage,
        run: () => window.dispatchEvent(new CustomEvent('studio-open-reference')),
      },
      {
        id: 'importLightroom',
        label: t('commands.importLightroom'),
        run: async () => {
          const catalog = await openDialog({ filters: [{ name: 'Lightroom Classic catalog', extensions: ['lrcat'] }] });
          if (typeof catalog !== 'string') return;
          const id = toast.loading(t('lightroomImport.working'));
          try {
            const r = await invoke<{ imported: number; missing: number; virtualCopiesSkipped: number; withDevelop: number }>(
              Invokes.ImportLightroomCatalog,
              { path: catalog },
            );
            toast.update(id, { render: t('lightroomImport.done', r), type: 'success', isLoading: false, autoClose: 8000 });
            window.dispatchEvent(new CustomEvent('studio-library-refresh'));
          } catch (err) {
            toast.update(id, { render: String(err), type: 'error', isLoading: false, autoClose: 8000 });
          }
        },
      },
      {
        id: 'cleanMode',
        label:
          useSettingsStore.getState().appSettings?.cleanMode === false
            ? t('commands.cleanModeOn')
            : t('commands.cleanModeOff'),
        run: () => {
          const settings = useSettingsStore.getState();
          if (!settings.appSettings) return;
          settings.handleSettingsChange({
            ...settings.appSettings,
            cleanMode: settings.appSettings.cleanMode === false,
          });
        },
      },
      { id: 'undo', label: t('commands.undo'), enabled: hasImage, run: () => editor.undo() },
      { id: 'redo', label: t('commands.redo'), enabled: hasImage, run: () => editor.redo() },
      {
        id: 'askSky',
        label: t('commands.askSky'),
        enabled: hasImage,
        run: () => askClaude(t('commands.askSkyPrompt')),
      },
      {
        id: 'askCinematic',
        label: t('commands.askCinematic'),
        enabled: hasImage,
        run: () => askClaude(t('commands.askCinematicPrompt')),
      },
    ].filter((c) => c.enabled !== false);
  }, [t, onLibrary, onOpenFolder, onHome, editorActions, open]);

  const matches = useMemo(() => {
    const q = query.trim().toLowerCase();
    return q ? commands.filter((c) => c.label.toLowerCase().includes(q)) : commands;
  }, [commands, query]);

  if (!open) return null;

  const run = (command?: Command) => {
    setOpen(false);
    command?.run();
  };

  return (
    <div
      className="absolute inset-0 z-[60] flex items-start justify-center pt-[12vh] bg-black/50"
      onClick={() => setOpen(false)}
    >
      <div
        className="w-[520px] max-w-[90vw] bg-bg-secondary border border-surface rounded-md shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 px-3 h-10 border-b border-surface">
          <Search size={14} className="text-text-secondary shrink-0" />
          <input
            ref={inputRef}
            className="flex-1 bg-transparent text-sm text-text-primary placeholder:text-text-secondary focus:outline-none"
            onChange={(e) => {
              setQuery(e.target.value);
              setCursor(0);
            }}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === 'ArrowDown') setCursor((c) => Math.min(c + 1, matches.length - 1));
              else if (e.key === 'ArrowUp') setCursor((c) => Math.max(c - 1, 0));
              else if (e.key === 'Enter') run(matches[cursor]);
              else if (e.key === 'Escape') setOpen(false);
            }}
            placeholder={t('commands.placeholder')}
            value={query}
          />
        </div>
        <div className="max-h-80 overflow-y-auto py-1">
          {matches.length === 0 && (
            <Text variant={TextVariants.small} color={TextColors.secondary} className="px-3 py-2">
              {t('commands.noMatch')}
            </Text>
          )}
          {matches.map((command, i) => (
            <button
              key={command.id}
              className={clsx(
                'w-full text-left px-3 py-2 text-sm flex items-center justify-between',
                i === cursor ? 'bg-card-active text-text-primary' : 'text-text-secondary hover:bg-surface',
              )}
              onMouseEnter={() => setCursor(i)}
              onClick={() => run(command)}
            >
              <span>{command.label}</span>
              {command.hint && <span className="text-[11px] text-text-secondary">{command.hint}</span>}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
