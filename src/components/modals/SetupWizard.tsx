import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import { Check, Cpu, Download, Laptop, LogIn, Sparkles, X } from 'lucide-react';
import clsx from 'clsx';

import { Invokes } from '../ui/AppProperties';
import Text from '../ui/Text';
import { TextColors, TextVariants } from '../../types/typography';
import { useSettingsStore } from '../../store/useSettingsStore';
import { Hardware, SetupAdvice, adviseSetup } from '../../utils/assistantSetup';

const OLLAMA_DOWNLOAD = 'https://ollama.com/download';
const CLAUDE_DOWNLOAD = 'https://claude.com/download';

type Choice = 'local' | 'claude' | 'both' | 'none';

/**
 * The one screen someone sees on a fresh install. It looks at the computer, says in plain words what it can do,
 * and sets the assistant up -- or leaves it out entirely, which is a real answer and not a failure. Nothing here
 * is permanent: the same choices live in Settings afterwards, and the screen says so.
 */
export default function SetupWizard() {
  const { t } = useTranslation();
  const appSettings = useSettingsStore((s) => s.appSettings);
  const handleSettingsChange = useSettingsStore((s) => s.handleSettingsChange);

  const [hardware, setHardware] = useState<Hardware | null>(null);
  const [advice, setAdvice] = useState<SetupAdvice | null>(null);
  const [choice, setChoice] = useState<Choice | null>(null);
  const [serverReady, setServerReady] = useState<boolean | null>(null);
  const [pull, setPull] = useState<{ percent: number; status: string; error?: string } | null>(null);
  const [claudeReady, setClaudeReady] = useState<boolean | null>(null);
  const [signingIn, setSigningIn] = useState(false);
  // Set once a local model is in place, so signing into Claude afterwards keeps both rather than replacing one.
  const [localModel, setLocalModel] = useState<string | null>(null);

  useEffect(() => {
    invoke<Hardware>(Invokes.DetectHardware)
      .then((hw) => {
        setHardware(hw);
        setAdvice(adviseSetup(hw));
      })
      .catch(() => setAdvice(null));
  }, []);

  useEffect(() => {
    const unlisten = listen<{ percent: number; status: string; done: boolean; error?: string }>(
      'local-model-pull',
      ({ payload }) => setPull(payload),
    );
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  if (!appSettings || appSettings.setupCompleted) return null;

  const checkServer = async () => {
    setServerReady(null);
    const status = await invoke<{ running: boolean }>(Invokes.LocalServerStatus, { url: null }).catch(() => ({
      running: false,
    }));
    setServerReady(status.running);
  };

  const checkClaude = async (finishWhenSignedIn = true) => {
    setClaudeReady(null);
    // Signed in or merely installed, both mean the Claude path can be offered; only a missing CLI cannot.
    const ok = await invoke<boolean>(Invokes.ClaudeAuthStatus).catch(() => null);
    setClaudeReady(ok !== null);
    if (ok && finishWhenSignedIn) finish('claude');
  };

  const pickLocal = async () => {
    setChoice('local');
    await checkServer();
  };

  const pickClaude = async () => {
    setChoice(localModel ? 'both' : 'claude');
    await checkClaude(!localModel);
  };

  const download = async () => {
    if (!advice?.local) return;
    setPull({ percent: 0, status: 'starting' });
    try {
      await invoke(Invokes.PullLocalModel, { model: advice.local.model, url: null });
      setLocalModel(advice.local.model);
      setChoice('both');
      await checkClaude(false);
    } catch (err) {
      setPull({ percent: 0, status: '', error: String(err) });
    }
  };

  const signIn = async () => {
    setSigningIn(true);
    try {
      await invoke(Invokes.ClaudeAuthLogin);
      const ok = await invoke<boolean>(Invokes.ClaudeAuthStatus).catch(() => false);
      if (ok) finish(localModel ? 'both' : 'claude', localModel ?? undefined);
    } finally {
      setSigningIn(false);
    }
  };

  const finish = (provider: Choice, localModel?: string) => {
    handleSettingsChange({
      ...appSettings,
      assistantProvider: provider,
      ...(localModel ? { localModelName: localModel, localModelUrl: 'http://127.0.0.1:11434/v1' } : {}),
      setupCompleted: true,
    });
  };

  const card = (key: 'local' | 'claude' | 'none', icon: React.ReactNode, onClick: () => void) => (
    <button
      key={key}
      onClick={onClick}
      className={clsx(
        'flex-1 text-left rounded-xl border p-4 transition-colors',
        choice === key ? 'border-accent bg-surface' : 'border-border-color hover:bg-surface',
      )}
    >
      <div className="flex items-center gap-2 mb-1 text-accent">
        {icon}
        <Text variant={TextVariants.heading}>{t(`setup.${key}.title`)}</Text>
        {advice?.suggested === key && (
          <span className="ml-auto text-[11px] rounded-full bg-accent text-button-text px-2 py-0.5">
            {t('setup.suggested')}
          </span>
        )}
      </div>
      <Text variant={TextVariants.small} color={TextColors.secondary}>
        {key === 'local' && advice?.local
          ? t('setup.local.desc', { model: advice.local.label, size: advice.local.downloadGb })
          : t(`setup.${key}.desc`)}
      </Text>
    </button>
  );

  return (
    <div className="fixed inset-0 z-[200] bg-black/70 backdrop-blur-sm flex items-center justify-center p-6">
      <div className="w-full max-w-2xl rounded-2xl bg-bg-primary border border-border-color shadow-2xl p-6 space-y-5">
        <div>
          <Text variant={TextVariants.title} color={TextColors.accent}>
            {t('setup.title')}
          </Text>
          <Text variant={TextVariants.small} color={TextColors.secondary}>
            {t('setup.subtitle')}
          </Text>
        </div>

        <div className="flex items-start gap-2 rounded-lg bg-surface p-3">
          <Cpu size={16} className="mt-0.5 text-text-secondary shrink-0" />
          <Text variant={TextVariants.small} color={TextColors.secondary}>
            {advice ? advice.reason : hardware ? t('setup.looking') : t('setup.looking')}
          </Text>
        </div>

        <div className="flex gap-3">
          {card('local', <Laptop size={16} />, pickLocal)}
          {card('claude', <Sparkles size={16} />, pickClaude)}
          {card('none', <X size={16} />, () => finish('none'))}
        </div>

        {choice === 'local' && (
          <div className="rounded-lg border border-border-color p-4 space-y-3">
            {serverReady === null && !pull && (
              <Text variant={TextVariants.small} color={TextColors.secondary}>
                {t('setup.local.checking')}
              </Text>
            )}
            {serverReady === false && (
              <>
                <Text variant={TextVariants.small} color={TextColors.secondary}>
                  {t('setup.local.needsOllama')}
                </Text>
                <div className="flex gap-2">
                  <a
                    className="rounded-md bg-accent text-button-text text-sm h-9 px-4 flex items-center"
                    href={OLLAMA_DOWNLOAD}
                    target="_blank"
                    rel="noreferrer"
                  >
                    {t('setup.local.getOllama')}
                  </a>
                  <button className="rounded-md bg-surface text-text-primary text-sm h-9 px-4" onClick={checkServer}>
                    {t('setup.local.installed')}
                  </button>
                </div>
              </>
            )}
            {serverReady && !pull && advice?.local && (
              <button
                className="flex items-center gap-2 rounded-md bg-accent text-button-text text-sm h-9 px-4"
                onClick={download}
              >
                <Download size={14} />
                {t('setup.local.download', { model: advice.local.label, size: advice.local.downloadGb })}
              </button>
            )}
            {pull && !pull.error && (
              <div className="space-y-1">
                <div className="h-2 rounded-full bg-surface overflow-hidden">
                  <div className="h-full bg-accent transition-all" style={{ width: `${pull.percent}%` }} />
                </div>
                <Text variant={TextVariants.small} color={TextColors.secondary}>
                  {t('setup.local.downloading', { percent: Math.round(pull.percent) })}
                </Text>
              </div>
            )}
            {pull?.error && (
              <Text variant={TextVariants.small} color={TextColors.secondary}>
                {pull.error}
              </Text>
            )}
          </div>
        )}

        {choice === 'both' && localModel && (
          <div className="rounded-lg border border-accent/40 bg-surface p-4 space-y-2">
            <Text variant={TextVariants.small} color={TextColors.secondary}>
              {t('setup.both.ready', { model: advice?.local?.label ?? localModel })}
            </Text>
          </div>
        )}

        {(choice === 'claude' || choice === 'both') && (
          <div className="rounded-lg border border-border-color p-4 space-y-3">
            {claudeReady === null && (
              <Text variant={TextVariants.small} color={TextColors.secondary}>
                {t('setup.claude.checking')}
              </Text>
            )}
            {claudeReady === false && (
              <>
                <Text variant={TextVariants.small} color={TextColors.secondary}>
                  {t('setup.claude.needsApp')}
                </Text>
                <div className="flex gap-2">
                  <a
                    className="rounded-md bg-accent text-button-text text-sm h-9 px-4 flex items-center"
                    href={CLAUDE_DOWNLOAD}
                    target="_blank"
                    rel="noreferrer"
                  >
                    {t('setup.claude.getApp')}
                  </a>
                  <button className="rounded-md bg-surface text-text-primary text-sm h-9 px-4" onClick={() => checkClaude()}>
                    {t('setup.local.installed')}
                  </button>
                </div>
              </>
            )}
            {claudeReady && (
              <button
                className="flex items-center gap-2 rounded-md bg-accent text-button-text text-sm h-9 px-4 disabled:opacity-50"
                disabled={signingIn}
                onClick={signIn}
              >
                <LogIn size={14} />
                {signingIn ? t('setup.claude.waiting') : t('setup.claude.signIn')}
              </button>
            )}
          </div>
        )}

        <div className="flex items-center justify-between pt-1">
          <Text variant={TextVariants.small} color={TextColors.secondary}>
            {t('setup.changeLater')}
          </Text>
          <button
            className="flex items-center gap-2 rounded-md bg-surface text-text-primary text-sm h-9 px-4"
            onClick={() => finish(choice ?? 'none', localModel ?? undefined)}
          >
            <Check size={14} />
            {t('setup.done')}
          </button>
        </div>
      </div>
    </div>
  );
}
