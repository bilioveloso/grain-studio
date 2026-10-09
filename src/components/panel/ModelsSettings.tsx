import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { Download, FolderOpen, Trash2 } from 'lucide-react';
import Button from '../ui/Button';
import Text from '../ui/Text';
import { TextColors, TextVariants } from '../../types/typography';
import { Invokes } from '../ui/AppProperties';

interface AiModelInfo {
  id: string;
  files: number;
  installedFiles: number;
  sizeBytes: number;
}

interface AiModelList {
  modelsDir: string;
  models: AiModelInfo[];
}

const formatSize = (bytes: number) =>
  bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.max(1, Math.round(bytes / 1e6))} MB`;

export default function ModelsSettings() {
  const { t } = useTranslation();
  const [list, setList] = useState<AiModelList | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [confirming, setConfirming] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    invoke<AiModelList>(Invokes.ListAiModels)
      .then(setList)
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(refresh, [refresh]);

  const run = async (id: string, command: Invokes) => {
    setBusy(id);
    setConfirming(null);
    setError(null);
    try {
      await invoke(command, { id });
    } catch (e) {
      setError(String(e));
    }
    setBusy(null);
    refresh();
  };

  const total = list?.models.reduce((sum, m) => sum + m.sizeBytes, 0) ?? 0;

  return (
    <div className="p-6 bg-surface rounded-xl shadow-md">
      <Text variant={TextVariants.title} color={TextColors.accent} className="mb-4">
        {t('settings.aiModels.title')}
      </Text>
      <Text className="mb-4">{t('settings.aiModels.description')}</Text>
      <div className="flex items-center justify-between mb-6">
        <Text variant={TextVariants.small}>{t('settings.aiModels.total', { size: formatSize(total) })}</Text>
        {list && (
          <Button onClick={() => invoke(Invokes.ShowInFinder, { path: list.modelsDir })}>
            <FolderOpen size={16} className="mr-2" />
            {t('settings.aiModels.openFolder')}
          </Button>
        )}
      </div>

      <div className="space-y-3">
        {list?.models.map((model) => {
          const installed = model.installedFiles === model.files;
          const status = installed
            ? formatSize(model.sizeBytes)
            : model.installedFiles > 0
              ? t('settings.aiModels.incomplete')
              : t('settings.aiModels.notDownloaded');
          return (
            <div key={model.id} className="flex items-center gap-4 p-3 rounded-lg bg-bg-primary">
              <div className="flex-1 min-w-0">
                <Text variant={TextVariants.heading}>
                  {t(`settings.aiModels.names.${model.id}` as 'settings.aiModels.names.masks')}
                </Text>
                <Text variant={TextVariants.small}>
                  {t(`settings.aiModels.uses.${model.id}` as 'settings.aiModels.uses.masks')}
                </Text>
              </div>
              <Text variant={TextVariants.small} className="whitespace-nowrap">
                {status}
              </Text>
              {busy === model.id ? (
                <Button disabled>{t('settings.aiModels.working')}</Button>
              ) : !installed ? (
                <Button disabled={busy !== null} onClick={() => run(model.id, Invokes.DownloadAiModel)}>
                  <Download size={16} className="mr-2" />
                  {t('settings.aiModels.download')}
                </Button>
              ) : confirming === model.id ? (
                <Button onClick={() => run(model.id, Invokes.RemoveAiModel)} onBlur={() => setConfirming(null)}>
                  <Trash2 size={16} className="mr-2" />
                  {t('settings.aiModels.confirmRemove')}
                </Button>
              ) : (
                <Button disabled={busy !== null} onClick={() => setConfirming(model.id)}>
                  <Trash2 size={16} className="mr-2" />
                  {t('settings.aiModels.remove')}
                </Button>
              )}
            </div>
          );
        })}
      </div>

      {error && (
        <Text variant={TextVariants.small} className="mt-4 text-red-400">
          {error}
        </Text>
      )}
    </div>
  );
}
