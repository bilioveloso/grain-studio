import { useCallback, useState, useEffect } from 'react';
import { platform } from '@tauri-apps/plugin-os';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Keyboard, Minus, Settings, Square, X } from 'lucide-react';
import clsx from 'clsx';
import { useTranslation } from 'react-i18next';
import { useSettingsStore } from '../store/useSettingsStore';
import { useUIStore } from '../store/useUIStore';

const RestoreDownIcon = ({ size = 14, className = '' }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
  >
    <rect x="3" y="6" width="8" height="8" rx="1.5" />
    <path d="M6 6V4.5A1.5 1.5 0 0 1 7.5 3h5A1.5 1.5 0 0 1 14 4.5v5A1.5 1.5 0 0 1 12.5 11H11" />
  </svg>
);

interface TitleBarProps {
  /// Lightroom-style modules: Library browses, Develop edits the selected photo.
  activeView?: string;
  canDevelop?: boolean;
  onLibrary?: () => void;
  onDevelop?: () => void;
}

export default function TitleBar({ activeView, canDevelop, onLibrary, onDevelop }: TitleBarProps = {}) {
  const { t } = useTranslation();
  const cleanModeOff = useSettingsStore((s) => s.appSettings?.cleanMode === false);
  const [osPlatform, setOsPlatform] = useState('');
  const [isMaximized, setIsMaximized] = useState(false);

  const appWindow = getCurrentWindow();

  useEffect(() => {
    const getPlatform = async () => {
      try {
        const p = platform();
        setOsPlatform(p);
      } catch (error) {
        console.error('Failed to get platform:', error);
        setOsPlatform('windows');
      }
    };
    getPlatform();
  }, []);

  useEffect(() => {
    const updateMaximizedState = async () => {
      try {
        const max = await appWindow.isMaximized();
        setIsMaximized(max);
      } catch (error) {
        console.error('Failed to check maximized state:', error);
      }
    };

    updateMaximizedState();

    let unlisten: () => void;
    appWindow
      .onResized(() => {
        updateMaximizedState();
      })
      .then((u) => (unlisten = u));

    return () => {
      if (unlisten) unlisten();
    };
  }, [appWindow]);

  const handleMinimize = () => appWindow.minimize();
  const handleClose = () => appWindow.close();

  const handleMaximize = useCallback(async () => {
    try {
      if (osPlatform === 'macos') {
        const isFullscreen = await appWindow.isFullscreen();
        appWindow.setFullscreen(!isFullscreen);
      } else {
        appWindow.toggleMaximize();
      }
    } catch (error) {
      console.error('Failed to toggle maximize:', error);
    }
  }, [osPlatform, appWindow]);

  const isMac = osPlatform === 'macos';
  const isLinux = osPlatform === 'linux';
  const isWindows = osPlatform === 'windows';
  const isMobile = osPlatform === 'android';
  if (!osPlatform || isMobile) {
    return null;
  }
  const outerDragProps = isLinux ? {} : { 'data-tauri-drag-region': 'true' };

  return (
    <div className="relative pt-2 px-2 w-full z-50 bg-transparent" {...outerDragProps}>
      <div
        className="h-9 bg-bg-secondary flex justify-between items-center select-none rounded-lg overflow-hidden"
        {...outerDragProps}
      >
        <div className="flex items-center h-full">
          {isMac && (
            <div className="flex items-center h-full px-4 space-x-2 z-10">
              <button
                aria-label="Close window"
                className="w-3 h-3 rounded-full bg-red-500 hover:bg-red-600 transition-colors duration-150"
                onClick={handleClose}
              />
              <button
                aria-label="Minimize window"
                className="w-3 h-3 rounded-full bg-yellow-500 hover:bg-yellow-600 transition-colors duration-150"
                onClick={handleMinimize}
              />
              <button
                aria-label="Maximize window"
                className="w-3 h-3 rounded-full bg-green-500 hover:bg-green-600 transition-colors duration-150"
                onClick={handleMaximize}
              />
            </div>
          )}
          <div data-tauri-drag-region className={`flex items-center h-full ${isMac ? '' : 'px-4'}`}>
            <p className="text-[13px] font-semibold text-text-secondary pointer-events-none">
              {t('library.splash.brand')}
            </p>
          </div>
          {cleanModeOff && (
            <button
              className="ml-2 px-2 h-5 rounded-full text-[10px] font-medium bg-accent/20 text-accent hover:bg-accent/30 z-10"
              onClick={() => {
                const settings = useSettingsStore.getState();
                if (settings.appSettings) settings.handleSettingsChange({ ...settings.appSettings, cleanMode: true });
              }}
              data-tooltip={t('titleBar.aiAllowedHint')}
            >
              {t('titleBar.aiAllowed')}
            </button>
          )}
          {onLibrary && onDevelop && (
            <div className="flex items-center gap-1 h-full pl-2 z-10">
              {[
                { id: 'library', label: t('modules.library'), go: onLibrary, enabled: true },
                { id: 'editor', label: t('modules.develop'), go: onDevelop, enabled: !!canDevelop },
              ].map((m) => (
                <button
                  key={m.id}
                  className={clsx(
                    'px-2.5 h-6 rounded text-[12px] transition-colors',
                    activeView === m.id ? 'text-accent bg-card-active' : 'text-text-secondary hover:text-text-primary',
                    !m.enabled && 'opacity-40 pointer-events-none',
                  )}
                  onClick={m.go}
                  data-tooltip={m.id === 'library' ? t('modules.libraryHint') : t('modules.developHint')}
                >
                  {m.label}
                </button>
              ))}
            </div>
          )}
        </div>
        <div data-tauri-drag-region className="flex-1 h-full" />
        <div className="flex items-center h-full z-10">
          <div className="flex items-center gap-0.5 pr-1">
            <button
              aria-label={t('titleBar.shortcuts')}
              className="w-8 h-7 rounded inline-flex justify-center items-center text-text-secondary hover:text-text-primary hover:bg-white/10 transition-colors"
              onClick={() => useUIStore.getState().setUI({ isSettingsOpen: true, settingsCategory: 'shortcuts' })}
              data-tooltip={t('titleBar.shortcuts')}
            >
              <Keyboard size={15} />
            </button>
            <button
              aria-label={t('titleBar.settings')}
              className="w-8 h-7 rounded inline-flex justify-center items-center text-text-secondary hover:text-text-primary hover:bg-white/10 transition-colors"
              onClick={() => useUIStore.getState().setUI({ isSettingsOpen: true, settingsCategory: null })}
              data-tooltip={t('titleBar.settings')}
            >
              <Settings size={15} />
            </button>
          </div>
          {isLinux && (
            <div className="flex items-center gap-2 pr-2 h-full">
              <button
                aria-label="Minimize window"
                className="w-7 h-7 rounded-full inline-flex justify-center items-center hover:bg-white/10 transition-colors duration-150"
                onClick={handleMinimize}
              >
                <Minus size={16} className="text-text-secondary" />
              </button>
              <button
                aria-label="Maximize window"
                className="w-7 h-7 rounded-full inline-flex justify-center items-center hover:bg-white/10 transition-colors duration-150"
                onClick={handleMaximize}
              >
                {isMaximized ? (
                  <RestoreDownIcon size={13} className="text-text-secondary" />
                ) : (
                  <Square size={13} className="text-text-secondary" />
                )}
              </button>
              <button
                aria-label="Close window"
                className="w-7 h-7 rounded-full inline-flex justify-center items-center hover:bg-red-500 hover:text-white transition-colors duration-150"
                onClick={handleClose}
              >
                <X size={16} className="text-text-secondary hover:text-white" />
              </button>
            </div>
          )}

          {isWindows && <div data-tauri-drag-region className="w-36 shrink-0 h-full pointer-events-none" />}
        </div>
      </div>

      {isWindows && (
        <div className="absolute top-0 right-0 flex h-12 z-20">
          <button aria-label="Minimize window" className="relative w-12 group outline-none" onClick={handleMinimize}>
            <div className="absolute bottom-0 left-0 w-12 h-10 flex justify-center items-center group-hover:bg-white/10 group-active:bg-white/20 transition-colors duration-150">
              <Minus size={16} className="text-text-secondary" />
            </div>
          </button>

          <button aria-label="Maximize window" className="relative w-12 group outline-none" onClick={handleMaximize}>
            <div className="absolute bottom-0 left-0 w-12 h-10 flex justify-center items-center group-hover:bg-white/10 group-active:bg-white/20 transition-colors duration-150">
              {isMaximized ? (
                <RestoreDownIcon size={12} className="text-text-secondary" />
              ) : (
                <Square size={12} className="text-text-secondary" />
              )}
            </div>
          </button>

          <button aria-label="Close window" className="relative w-14 group outline-none" onClick={handleClose}>
            <div className="absolute bottom-0 left-0 w-12 h-10 flex justify-center items-center group-hover:bg-red-500 group-active:bg-red-600 transition-colors duration-150 rounded-r-lg">
              <X size={16} className="text-text-secondary group-hover:text-white transition-colors duration-150" />
            </div>
          </button>
        </div>
      )}
    </div>
  );
}
