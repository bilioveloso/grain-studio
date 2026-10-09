import { useTranslation } from 'react-i18next';
import { Images, MousePointerClick, Command } from 'lucide-react';

import Text from '../../ui/Text';
import { TextColors, TextVariants } from '../../../types/typography';
import { useLibraryStore } from '../../../store/useLibraryStore';

/**
 * Shown in the right column while no photo is open, instead of "No image selected":
 * what this folder holds and how to get going.
 */
export default function LibrarySummary() {
  const { t } = useTranslation();
  const imageList = useLibraryStore((s) => s.imageList);
  const multiSelectedPaths = useLibraryStore((s) => s.multiSelectedPaths);
  const currentFolderPath = useLibraryStore((s) => s.currentFolderPath);

  const folderName = currentFolderPath?.split(/[\\/]/).filter(Boolean).pop() ?? '';
  const rows = [
    { icon: Images, label: t('library.summary.photos'), value: String(imageList?.length ?? 0) },
    { icon: MousePointerClick, label: t('library.summary.selected'), value: String(multiSelectedPaths?.length ?? 0) },
  ];

  return (
    <div className="p-3 flex flex-col gap-3">
      {folderName && (
        <Text variant={TextVariants.heading} className="truncate">
          {folderName}
        </Text>
      )}
      <div className="flex flex-col gap-1.5">
        {rows.map(({ icon: Icon, label, value }) => (
          <div key={label} className="flex items-center gap-2">
            <Icon size={13} className="text-text-secondary shrink-0" />
            <Text variant={TextVariants.small} color={TextColors.secondary} className="flex-1">
              {label}
            </Text>
            <Text variant={TextVariants.small}>{value}</Text>
          </div>
        ))}
      </div>
      <div className="flex items-start gap-2 pt-1 border-t border-surface">
        <Command size={13} className="text-text-secondary shrink-0 mt-1" />
        <Text variant={TextVariants.small} color={TextColors.secondary}>
          {t('library.summary.hint')}
        </Text>
      </div>
    </div>
  );
}
