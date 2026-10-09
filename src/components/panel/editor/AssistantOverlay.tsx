import { useEffect } from 'react';
import { MapPin, Lasso, Eraser } from 'lucide-react';
import clsx from 'clsx';
import { useTranslation } from 'react-i18next';

import { useEditorStore } from '../../../store/useEditorStore';
import { useSettingsStore } from '../../../store/useSettingsStore';
import ClaudePanel from '../right/ClaudePanel';

/**
 * The assistant, over the photo instead of squeezed into the side panel: a toolbar on top for marking places,
 * the chat under the photo. The sliders stay in the right panel, so you still watch them move while it edits.
 */
export default function AssistantOverlay() {
  const { t } = useTranslation();
  const annotateMode = useEditorStore((s) => s.annotateMode);
  const annotateTool = useEditorStore((s) => s.annotateTool);
  const annotations = useEditorStore((s) => s.annotations);
  const setEditor = useEditorStore((s) => s.setEditor);
  // Someone who chose to edit without an assistant should not see its furniture over their photo.
  const assistant = useSettingsStore((s) => s.appSettings?.assistantProvider);

  // Esc leaves marking without losing what is already marked.
  useEffect(() => {
    if (!annotateMode) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setEditor({ annotateMode: false });
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [annotateMode, setEditor]);

  if (assistant === 'none') return null;

  return (
    <>
      <div className="pointer-events-none absolute top-3 inset-x-0 z-30 flex justify-center" data-assistant-overlay>
        <div className="pointer-events-auto flex items-center gap-1 rounded-full border border-border-color bg-surface/95 backdrop-blur-md shadow-xl px-1.5 py-1">
          {(
            [
              { tool: 'pin', icon: MapPin, label: t('claude.annotate.mark'), tip: t('claude.annotate.tooltip') },
              { tool: 'ring', icon: Lasso, label: t('claude.annotate.circle'), tip: t('claude.annotate.circleTooltip') },
            ] as const
          ).map(({ tool, icon: Icon, label, tip }) => {
            const on = annotateMode && annotateTool === tool;
            return (
              <button
                key={tool}
                className={clsx(
                  'flex items-center gap-2 rounded-full px-3 py-1.5 text-sm transition-colors',
                  on ? 'bg-accent text-button-text' : 'text-text-primary hover:bg-bg-primary',
                )}
                // Clicking the tool you are already using puts the marking tools away.
                onClick={() => setEditor({ annotateMode: !on, annotateTool: tool })}
                data-tooltip={tip}
              >
                <Icon size={14} />
                {label}
              </button>
            );
          })}
          {/* Always here, so clearing marks never shifts the tools out from under the pointer. */}
          <button
            className="flex items-center gap-2 rounded-full px-3 py-1.5 text-sm text-text-secondary hover:text-text-primary hover:bg-bg-primary disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-text-secondary"
            disabled={annotations.length === 0}
            onClick={() => setEditor({ annotations: [] })}
          >
            <Eraser size={14} />
            {t('claude.annotate.clear')}
          </button>
        </div>
      </div>

      <div className="pointer-events-none absolute inset-x-0 bottom-0 z-30 flex justify-center p-3" data-assistant-overlay>
        <ClaudePanel />
      </div>
    </>
  );
}
