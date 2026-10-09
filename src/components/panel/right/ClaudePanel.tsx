import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { ChevronDown, ChevronUp, LogIn, RotateCcw, SendHorizontal, Sparkles, Wrench } from 'lucide-react';
import clsx from 'clsx';

import { Invokes } from '../../ui/AppProperties';
import Text from '../../ui/Text';
import { TextColors, TextVariants } from '../../../types/typography';
import { useEditorStore } from '../../../store/useEditorStore';
import { Annotation, composeAnnotatedMessage, padBox, ringBox } from '../../../utils/annotations';
import { parseVerbEdit, applyVerbEdit } from '../../../utils/verbEdits';
import { debouncedSetHistory } from '../../../hooks/useEditorActions';
import { Adjustments, Coord, INITIAL_MASK_CONTAINER, MaskContainer } from '../../../utils/adjustments';
import { createSubMask, measureCut, shouldKeepDrawnShape } from '../../../utils/maskUtils';
import { useAiMasking } from '../../../hooks/useAiMasking';
import { previewToImage } from '../../../hooks/useStudioBridge';
import { Mask, SubMask, SubMaskMode } from './Masks';
import { v4 as uuidv4 } from 'uuid';
import { useSettingsStore } from '../../../store/useSettingsStore';

type Role = 'user' | 'assistant' | 'tool' | 'system';

interface ChatLine {
  id: number;
  role: Role;
  text: string;
}

let nextId = 0;

// Tool names arrive as mcp__studio__set_adjustments; show just the action.
const toolLabel = (name: string) => name.replace(/^mcp__studio__/, '').replace(/_/g, ' ');

// claude-sonnet-5 -> Sonnet 5, claude-haiku-4-5-20251001 -> Haiku 4.5.
const modelLabel = (id: string) => {
  const parts = id.replace(/^claude-/, '').replace(/-\d{8}$/, '').split('-');
  const name = parts.shift() || id;
  const version = parts.join('.');
  return name.charAt(0).toUpperCase() + name.slice(1) + (version ? ` ${version}` : '');
};

const EMPTY_USAGE = { tokens: 0, costUsd: 0, turns: 0 };

/**
 * Turns every drawn ring into a real mask on the photo and returns their ids by annotation id.
 *
 * The ring says *where*, but it is not the shape of the thing inside it: circle a crane and the ring also holds
 * the sky between its struts. So the mask is the subject-detection model's cut of what is in there, intersected
 * with the ring -- the model finds the object, the ring stops it wandering off. The model runs on this computer,
 * so this costs nothing and asks nobody. If it has nothing to offer, the drawn shape is used as it was drawn.
 */
async function masksFromRings(
  marks: Annotation[],
  name: (n: number) => string,
  findSubject: (subMaskId: string, start: Coord, end: Coord, points: Array<Coord & { label?: number }>) => Promise<void>,
): Promise<Record<string, string>> {
  const rings = marks.filter((a) => a.kind === 'ring' && a.points.length >= 3);
  if (!rings.length) return {};
  const { selectedImage, adjustments } = useEditorStore.getState();
  if (!selectedImage?.isReady) return {};

  const size = { width: selectedImage.width, height: selectedImage.height };
  const toImage = previewToImage(selectedImage, adjustments);
  const made: Record<string, string> = {};
  const seeds: Array<{ subjectId: string; ringId: string; start: Coord; end: Coord; points: Array<Coord & { label?: number }> }> = [];

  const containers = rings.map((ring, i) => {
    // The subject the model finds, then the drawn ring intersected over it to keep the result where it was drawn.
    const subject = createSubMask(Mask.AiSubject, size, SubMaskMode.Additive);
    const base = createSubMask(Mask.Polygon, size, SubMaskMode.Intersect);
    const shape = {
      ...base,
      mode: SubMaskMode.Intersect,
      // Drawn by hand, so let the photo's own edges tidy the line; the slider in the Masks panel can undo it.
      parameters: { ...base.parameters, points: ring.points.map((p) => toImage([p.x, p.y])), refine: 60 },
    } as unknown as SubMask;
    const [x1, y1, x2, y2] = padBox(ringBox(ring.points));
    seeds.push({
      subjectId: subject.id,
      ringId: ring.id,
      start: toImage([x1, y1]),
      end: toImage([x2, y2]),
      // No point prompts. The circle is already a box, and that is the prompt the model is best at: given the
      // box alone it returns the whole stork, beak to toes. Add points picked out of the circle -- however they
      // are chosen, spread out, clustered in the middle, walked down the long axis -- and it narrows to whatever
      // those points have in common, which is a part: the head and one wing, or a strip down the legs.
      points: [],
    });
    const container: MaskContainer = {
      ...INITIAL_MASK_CONTAINER,
      id: uuidv4(),
      name: name((adjustments.masks?.length ?? 0) + i + 1),
      subMasks: [subject, shape],
    };
    made[ring.id] = container.id;
    return container;
  });

  useEditorStore.getState().setEditor((state) => ({
    adjustments: { ...state.adjustments, masks: [...(state.adjustments.masks ?? []), ...containers] },
  }));

  for (const seed of seeds) {
    await findSubject(seed.subjectId, seed.start, seed.end, seed.points).catch(() => {});
    const cut = useEditorStore
      .getState()
      .adjustments.masks?.flatMap((m: MaskContainer) => m.subMasks)
      .find((sm: SubMask) => sm.id === seed.subjectId);
    const bitmap = cut?.parameters?.maskDataBase64;
    if (bitmap) {
      // It returned something, which is not the same as it being any use. Measure how much of the circle it
      // actually filled: a speck or the whole box both mean the drawn shape is the better answer.
      const cutShape = await measureCut(bitmap, {
        x: seed.start.x,
        y: seed.start.y,
        w: seed.end.x - seed.start.x,
        h: seed.end.y - seed.start.y,
      }).catch(() => ({ coverage: NaN, reach: NaN }));
      if (!shouldKeepDrawnShape(cutShape)) continue;
    }
    // Nothing usable came back: drop the subject, or intersecting with it would leave an empty mask.
    useEditorStore.getState().setEditor((state) => ({
      adjustments: {
        ...state.adjustments,
        masks: (state.adjustments.masks ?? []).map((m: MaskContainer) =>
          m.id === made[seed.ringId]
            ? {
                ...m,
                subMasks: m.subMasks
                  .filter((sm: SubMask) => sm.id !== seed.subjectId)
                  .map((sm: SubMask) => ({ ...sm, mode: SubMaskMode.Additive })),
              }
            : m,
        ),
      },
    }));
  }

  return made;
}

/**
 * Does the edit here when the words are plain enough for the app to be sure of them, and says so in the
 * transcript. Returns false when anything needs reading rather than matching, which sends it to the assistant
 * as before. Marks with a mask of their own are changed inside that mask; everything else changes the photo.
 */
function applyPlainEdits(
  marks: Annotation[],
  masks: Record<string, string>,
  input: string,
  push: (role: Role, text: string) => void,
  t: TFunction,
): boolean {
  const instructions = marks.length ? marks.map((m) => ({ mark: m, text: m.note.trim() || input.trim() })) : [];
  const edits = marks.length
    ? instructions.map((i) => parseVerbEdit(i.text))
    : [parseVerbEdit(input)];
  // All of them or none: a message where one part is plain and another is not belongs to the assistant whole.
  if (!edits.length || edits.some((e) => !e)) return false;

  const { adjustments } = useEditorStore.getState();
  let next: Adjustments = { ...adjustments };
  const done: string[] = [];

  edits.forEach((edit, i) => {
    const maskId = marks.length ? masks[instructions[i].mark.id] : undefined;
    if (maskId) {
      next = {
        ...next,
        masks: (next.masks ?? []).map((m: MaskContainer) =>
          m.id === maskId ? { ...m, adjustments: applyVerbEdit(m.adjustments, edit!.deltas) } : m,
        ),
      };
      done.push(t('claude.plainEdit.inMark', { what: edit!.label, number: i + 1 }));
    } else {
      next = applyVerbEdit(next, edit!.deltas);
      done.push(t('claude.plainEdit.wholePhoto', { what: edit!.label }));
    }
  });

  useEditorStore.getState().setEditor({ adjustments: next });
  debouncedSetHistory(next);
  push('user', instructions.length ? instructions.map((i, n) => `${n + 1}. ${i.text}`).join('\n') : input.trim());
  push('system', `${done.join(' ')} ${t('claude.plainEdit.noAssistant')}`);
  return true;
}

/**
 * Claude chat docked under the adjustment sliders, so edits Claude makes are visible
 * on the sliders while you talk. Collapses to a slim bar; the conversation stays mounted.
 */
export default function ClaudePanel() {
  const { t } = useTranslation();
  // The subject-detection model that turns a drawn ring into the thing inside it. Runs on this computer.
  const { handleGenerateAiMask } = useAiMasking();
  const [open, setOpen] = useState(false);
  const [lines, setLines] = useState<ChatLine[]>([]);
  const [input, setInput] = useState('');
  const [busy, setBusy] = useState(false);
  // Sign-in: null = unknown yet. `claude auth login` opens the browser and may ask for a code.
  const [signedIn, setSignedIn] = useState<boolean | null>(null);
  // What the CLI reports about itself: the model it chose, and what this conversation has used.
  const [model, setModel] = useState('');
  const [usage, setUsage] = useState(EMPTY_USAGE);
  const [authRunning, setAuthRunning] = useState(false);
  const [authUrl, setAuthUrl] = useState('');
  const [authCode, setAuthCode] = useState('');
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Local models need no sign-in and aren't Claude, so the panel names itself after the provider.
  const configured = useSettingsStore((s) => s.appSettings?.assistantProvider) || 'claude';
  const bothEngines = configured === 'both';
  // With both set up, each message goes to whichever is chosen here; the free one answers unless asked otherwise.
  const [engine, setEngine] = useState<'local' | 'claude'>(bothEngines ? 'local' : 'claude');
  const provider = bothEngines ? engine : configured;
  const name = provider === 'local' ? t('claude.localName') : 'Claude';
  // Claude Code reports the model it picked; a local server only has the name you set.
  const localModel = useSettingsStore((s) => s.appSettings?.localModelName);
  const shownModel = provider === 'local' ? localModel : model && modelLabel(model);
  const claudeModel = useSettingsStore((s) => s.appSettings?.claudeModel) || 'sonnet';
  // A different provider or model means a different session: end the running one so the next
  // message uses the new choice, and start the running totals again.
  const choice = `${provider}:${claudeModel}`;
  const lastChoice = useRef(choice);
  useEffect(() => {
    setSignedIn(null);
    if (lastChoice.current === choice) return;
    lastChoice.current = choice;
    setModel('');
    setUsage(EMPTY_USAGE);
    invoke(Invokes.ClaudeChatReset).catch(() => {});
    push('system', t('claude.modelSwitched'));
  }, [choice]);

  const push = (role: Role, text: string) => setLines((prev) => [...prev, { id: nextId++, role, text }]);

  useEffect(() => {
    const unlistenEvent = listen<any>('claude-chat-event', ({ payload: event }) => {
      switch (event.type) {
        case 'system':
          if (event.subtype === 'init') {
            if (event.model) setModel(event.model);
            const studio = event.mcp_servers?.find((s: any) => s.name === 'studio');
            if (studio && studio.status !== 'connected' && studio.status !== 'pending') {
              push('system', t('claude.bridgeOffline'));
            }
          }
          break;
        case 'assistant':
          for (const block of event.message?.content ?? []) {
            if (block.type === 'text' && block.text?.trim()) push('assistant', block.text);
            if (block.type === 'tool_use') push('tool', toolLabel(block.name));
          }
          break;
        case 'result': {
          setBusy(false);
          const used = event.usage ?? {};
          const tokens =
            (used.input_tokens ?? 0) +
            (used.output_tokens ?? 0) +
            (used.cache_creation_input_tokens ?? 0) +
            (used.cache_read_input_tokens ?? 0);
          setUsage((prev) => ({
            tokens: prev.tokens + tokens,
            costUsd: prev.costUsd + (event.total_cost_usd ?? 0),
            turns: prev.turns + 1,
          }));
          if (event.is_error) {
            push('system', event.result || t('claude.error', { name }));
            if (/log ?in|authenticat/i.test(String(event.result))) setSignedIn(false);
          }
          break;
        }
        case 'stderr':
          if (event.text?.trim()) console.warn('[claude]', event.text);
          break;
      }
    });
    const unlistenExit = listen('claude-chat-exit', () => {
      setBusy(false);
      push('system', t('claude.sessionEnded'));
    });
    const unlistenAuthOut = listen<{ text: string }>('claude-auth-output', ({ payload }) => {
      const url = payload.text.match(/https:\/\/\S+/)?.[0];
      if (url) setAuthUrl(url);
    });
    const unlistenAuthExit = listen<{ ok: boolean }>('claude-auth-exit', async ({ payload }) => {
      setAuthRunning(false);
      setAuthCode('');
      const ok = await invoke<boolean>(Invokes.ClaudeAuthStatus).catch(() => false);
      setSignedIn(ok);
      push('system', ok && payload.ok ? t('claude.signedIn') : t('claude.signInFailed'));
    });
    return () => {
      unlistenEvent.then((fn) => fn());
      unlistenExit.then((fn) => fn());
      unlistenAuthOut.then((fn) => fn());
      unlistenAuthExit.then((fn) => fn());
    };
  }, [t]);

  useEffect(() => {
    if (open) scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [lines, busy, open]);

  useEffect(() => {
    if (open) inputRef.current?.focus();
  }, [open]);

  useEffect(() => {
    if (open && signedIn === null)
      invoke<boolean>(Invokes.ClaudeAuthStatus)
        .then(setSignedIn)
        .catch((err: any) => {
          // e.g. Claude Code isn't installed: say so instead of just offering a sign-in button.
          setSignedIn(false);
          push('system', String(err));
        });
  }, [open, signedIn]);

  const signIn = async () => {
    setAuthUrl('');
    setAuthRunning(true);
    try {
      await invoke(Invokes.ClaudeAuthLogin);
    } catch (err: any) {
      setAuthRunning(false);
      push('system', String(err));
    }
  };

  const submitCode = async () => {
    if (!authCode.trim()) return;
    try {
      await invoke(Invokes.ClaudeAuthCode, { code: authCode });
    } catch (err: any) {
      push('system', String(err));
    }
  };

  const sendText = async (text: string, shown?: string) => {
    setOpen(true);
    push('user', shown ?? text);
    setBusy(true);
    try {
      await invoke(Invokes.ClaudeChatSend, { text, provider: bothEngines ? engine : null });
    } catch (err: any) {
      setBusy(false);
      push('system', String(err));
    }
  };

  const send = async () => {
    if (busy) return;
    const marks = useEditorStore.getState().annotations.filter((a) => a.note.trim() || input.trim());
    // A drawn ring is already the selection, so the app builds that mask itself: the assistant is handed a
    // finished mask and only decides what to change inside it, instead of hunting for the shape with a model.
    const masks = await masksFromRings(marks, (number) => t('claude.annotate.maskName', { number }), handleGenerateAiMask);
    // "darker", "a touch warmer": the app knows these, so it does them itself, for nothing and at once.
    if (applyPlainEdits(marks, masks, input, push, t)) {
      setInput('');
      useEditorStore.getState().setEditor({ annotations: [], annotateMode: false });
      return;
    }
    const text = composeAnnotatedMessage(input, marks, masks);
    if (!text) return;
    // The transcript shows what the user said and where, not the instructions we wrap around it.
    const extra = input.trim() && marks.some((m) => m.note.trim()) ? `\n${input.trim()}` : '';
    const shown = marks.length
      ? marks.map((m, i) => `${i + 1}. ${m.note.trim() || input.trim()}`).join('\n') + extra
      : input.trim();
    setInput('');
    if (marks.length) useEditorStore.getState().setEditor({ annotations: [], annotateMode: false });
    await sendText(text, shown);
  };

  const sendRef = useRef(sendText);
  sendRef.current = sendText;
  const pendingRequest = useEditorStore((s) => s.pendingClaudeRequest);
  useEffect(() => {
    if (!pendingRequest) return;
    // Clear first so a re-render (or StrictMode's double effect) can't send it twice.
    useEditorStore.getState().setEditor({ pendingClaudeRequest: null });
    sendRef.current(pendingRequest);
  }, [pendingRequest]);

  const reset = async () => {
    await invoke(Invokes.ClaudeChatReset);
    setLines([]);
    setBusy(false);
    setUsage(EMPTY_USAGE);
  };

  // A conversation is about one photo: everything said about the last one would be re-read on every step of the
  // next, which costs more the longer it goes on and confuses the answers. Opening another photo starts fresh.
  const openPhoto = useEditorStore((s) => s.selectedImage?.path);
  const lastPhoto = useRef(openPhoto);
  useEffect(() => {
    if (lastPhoto.current === openPhoto) return;
    lastPhoto.current = openPhoto;
    if (!lines.length) return;
    invoke(Invokes.ClaudeChatReset).catch(() => {});
    setLines([]);
    setUsage(EMPTY_USAGE);
    setBusy(false);
  }, [openPhoto, lines.length]);

  const marks = useEditorStore((s) => s.annotations);

  return (
    <div className="pointer-events-auto w-full max-w-3xl rounded-2xl border border-border-color bg-surface/95 backdrop-blur-md shadow-2xl flex flex-col overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-1.5">
        <button
          className="flex items-center gap-2 grow text-left text-text-primary hover:text-accent transition-colors"
          onClick={() => setOpen((v) => !v)}
          aria-expanded={open}
        >
          <Sparkles size={16} className={clsx(busy && 'animate-pulse text-accent')} />
          <Text variant={TextVariants.heading}>{t('claude.title', { name })}</Text>
          {shownModel && (
            <Text variant={TextVariants.small} color={TextColors.secondary}>
              {shownModel}
            </Text>
          )}
          {busy && (
            <Text variant={TextVariants.small} color={TextColors.secondary}>
              {t('claude.thinking', { name })}
            </Text>
          )}
          <span className="ml-auto text-text-secondary">
            {open ? <ChevronDown size={16} /> : <ChevronUp size={16} />}
          </span>
        </button>
        {bothEngines && (
          <button
            className="shrink-0 rounded-full border border-border-color px-2 py-0.5 text-xs text-text-secondary hover:text-text-primary hover:bg-bg-primary"
            onClick={() => setEngine(engine === 'local' ? 'claude' : 'local')}
            data-tooltip={t('claude.engine.switch')}
          >
            {engine === 'local' ? t('claude.engine.local') : t('claude.engine.claude')}
          </button>
        )}
        {provider !== 'local' && usage.turns > 0 && (
          <span data-tooltip={t('claude.costTooltip')} className="flex items-center gap-2">
            <Text variant={TextVariants.small} color={TextColors.secondary}>
              {t('claude.tokensUsed', { tokens: Math.round(usage.tokens / 1000).toLocaleString() })}
            </Text>
            <Text variant={TextVariants.small} color={TextColors.secondary}>
              {t('claude.costEstimate', { cost: usage.costUsd.toFixed(2) })}
            </Text>
          </span>
        )}
        <button
          className="p-1.5 rounded-md text-text-secondary hover:text-text-primary hover:bg-bg-primary"
          onClick={reset}
          data-tooltip={t('claude.newChat')}
        >
          <RotateCcw size={14} />
        </button>
      </div>

      {open && (
        <div className="flex flex-col max-h-[38vh] border-t border-border-color">
          {signedIn === false && (
            <div className="p-3 space-y-2">
              <Text variant={TextVariants.small} color={TextColors.secondary}>
                {t('claude.signInDesc')}
              </Text>
              <button
                className="w-full flex items-center justify-center gap-2 rounded-md bg-accent text-button-text text-sm h-8 disabled:opacity-50"
                disabled={authRunning}
                onClick={signIn}
              >
                <LogIn size={14} />
                {authRunning ? t('claude.signingIn') : t('claude.signIn')}
              </button>
              {authRunning && authUrl && (
                <a className="block text-xs text-accent hover:underline" href={authUrl} target="_blank" rel="noreferrer">
                  {t('claude.openSignInPage')}
                </a>
              )}
              {authRunning && (
                <div className="flex gap-2">
                  <input
                    className="flex-1 min-w-0 rounded-md border border-surface bg-bg-primary px-2 h-8 text-xs text-text-primary focus:border-accent"
                    onChange={(e) => setAuthCode(e.target.value)}
                    onKeyDown={(e) => {
                      e.stopPropagation();
                      if (e.key === 'Enter') submitCode();
                    }}
                    placeholder={t('claude.pasteCode')}
                    value={authCode}
                  />
                  <button
                    className="px-3 rounded-md bg-surface text-text-primary text-xs disabled:opacity-50"
                    disabled={!authCode.trim()}
                    onClick={submitCode}
                  >
                    {t('claude.submitCode')}
                  </button>
                </div>
              )}
            </div>
          )}
          <div ref={scrollRef} className="flex-1 overflow-y-auto p-3 space-y-2 custom-scrollbar">
            {lines.length === 0 && (
              <Text variant={TextVariants.small} color={TextColors.secondary}>
                {t('claude.empty', { name })}
              </Text>
            )}
            {lines.map((line) =>
              line.role === 'tool' ? (
                <div key={line.id} className="flex items-center gap-2 text-xs text-text-secondary">
                  <Wrench size={12} />
                  <span>{line.text}</span>
                </div>
              ) : (
                <div
                  key={line.id}
                  className={clsx(
                    'rounded-lg px-3 py-2 text-sm whitespace-pre-wrap break-words',
                    line.role === 'user' && 'bg-accent text-button-text ml-12',
                    line.role === 'assistant' && 'bg-bg-primary text-text-primary mr-12',
                    line.role === 'system' && 'text-text-secondary italic text-xs',
                  )}
                >
                  {line.text}
                </div>
              ),
            )}
          </div>
        </div>
      )}

      <div className="p-2 flex gap-2 items-end border-t border-border-color">
        {marks.length > 0 && (
          <span className="shrink-0 self-center rounded-full bg-accent/20 text-accent text-xs px-2 py-1">
            {t('claude.annotate.marked', { count: marks.length })}
          </span>
        )}
        <textarea
          ref={inputRef}
          className="flex-1 resize-none rounded-xl border border-surface bg-bg-primary px-3 py-2 text-sm text-text-primary focus:border-accent focus:outline-none"
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            // Keep editor shortcuts (B, D, arrows…) from firing while typing.
            e.stopPropagation();
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              send();
            }
          }}
          placeholder={marks.length ? t('claude.annotate.placeholder') : t('claude.placeholder', { name })}
          rows={1}
          value={input}
        />
        <button
          className="p-2 rounded-xl bg-accent text-button-text disabled:opacity-50"
          disabled={busy || (!input.trim() && !marks.some((m) => m.note.trim()))}
          onClick={send}
          data-tooltip={t('claude.send')}
        >
          <SendHorizontal size={16} />
        </button>
      </div>
    </div>
  );
}
