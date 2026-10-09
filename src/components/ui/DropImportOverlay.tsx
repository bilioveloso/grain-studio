import { useTranslation } from 'react-i18next';
import { ImageDown } from 'lucide-react';
import clsx from 'clsx';

import Text from './Text';
import { TextColors, TextVariants } from '../../types/typography';
import { useDropImport } from '../../hooks/useDropImport';
import { useLibraryStore } from '../../store/useLibraryStore';

/** Full-window hint while photos are dragged over the app. Rendered once in App. */
export default function DropImportOverlay() {
  const { t } = useTranslation();
  const { isDragging } = useDropImport();
  const currentFolderPath = useLibraryStore((s) => s.currentFolderPath);
  const folderName = currentFolderPath?.split(/[\\/]/).filter(Boolean).pop();

  return (
    <div
      className={clsx(
        'fixed inset-0 z-[60] flex items-center justify-center bg-black/50 backdrop-blur-xs transition-opacity duration-150 pointer-events-none',
        isDragging ? 'opacity-100' : 'opacity-0',
      )}
      aria-hidden={!isDragging}
    >
      <div className="flex flex-col items-center gap-3 rounded-xl border-2 border-dashed border-accent bg-surface/90 px-10 py-8">
        <ImageDown size={40} className="text-accent" />
        <Text variant={TextVariants.title}>
          {folderName ? t('dropImport.hint', { folder: folderName }) : t('dropImport.noFolder')}
        </Text>
        {folderName && (
          <Text variant={TextVariants.small} color={TextColors.secondary}>
            {t('dropImport.subhint')}
          </Text>
        )}
      </div>
    </div>
  );
}
