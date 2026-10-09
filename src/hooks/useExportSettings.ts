import { useState, useMemo, useCallback } from 'react';
import { ExportPreset, TiffBitDepth, WatermarkAnchor } from '../components/ui/ExportImportProperties';

export function useExportSettings() {
  const [fileFormat, setFileFormat] = useState('jpeg');
  const [jpegQuality, setJpegQuality] = useState(90);
  const [tiffBitDepth, setTiffBitDepth] = useState<TiffBitDepth>(16);
  const [enableResize, setEnableResize] = useState(false);
  const [resizeMode, setResizeMode] = useState('longEdge');
  const [resizeValue, setResizeValue] = useState(2048);
  const [dontEnlarge, setDontEnlarge] = useState(true);
  const [outputSharpening, setOutputSharpening] = useState(0);
  const [upscaleMethod, setUpscaleMethod] = useState('classic');
  const [keepMetadata, setKeepMetadata] = useState(true);
  const [preserveTimestamps, setPreserveTimestamps] = useState(false);
  const [stripGps, setStripGps] = useState(true);
  const [exportMasks, setExportMasks] = useState(false);
  // Which mask to cut the photo out with; empty means export the whole frame as usual.
  const [cutoutMaskId, setCutoutMaskId] = useState('');
  const [preserveFolders, setPreserveFolders] = useState(false);
  const [filenameTemplate, setFilenameTemplate] = useState('{original_filename}_edited');
  const [enableWatermark, setEnableWatermark] = useState(false);
  const [watermarkPath, setWatermarkPath] = useState<string | null>(null);
  const [watermarkAnchor, setWatermarkAnchor] = useState<WatermarkAnchor>(WatermarkAnchor.BottomRight);
  const [watermarkScale, setWatermarkScale] = useState(10);
  const [watermarkSpacing, setWatermarkSpacing] = useState(5);
  const [watermarkOpacity, setWatermarkOpacity] = useState(75);
  const [destinationType, setDestinationType] = useState<string>('customFolder');
  const [subfolder, setSubfolder] = useState<string>('');

  const handleApplyPreset = useCallback((preset: ExportPreset) => {
    setFileFormat(preset.fileFormat);
    setJpegQuality(preset.jpegQuality);
    setTiffBitDepth(preset.tiffBitDepth ?? 16);
    setEnableResize(preset.enableResize);
    setResizeMode(preset.resizeMode);
    setResizeValue(preset.resizeValue);
    setDontEnlarge(preset.dontEnlarge);
    setOutputSharpening(preset.outputSharpening ?? 0);
    setUpscaleMethod(preset.upscaleMethod ?? 'classic');
    setKeepMetadata(preset.keepMetadata);
    setPreserveTimestamps(preset.preserveTimestamps ?? false);
    setStripGps(preset.stripGps);
    setExportMasks(preset.exportMasks ?? false);
    setCutoutMaskId(preset.cutoutMaskId ?? '');
    setPreserveFolders(preset.preserveFolders ?? false);
    setFilenameTemplate(preset.filenameTemplate);
    setEnableWatermark(preset.enableWatermark);
    setWatermarkPath(preset.watermarkPath);
    setWatermarkAnchor(preset.watermarkAnchor as WatermarkAnchor);
    setWatermarkScale(preset.watermarkScale);
    setWatermarkSpacing(preset.watermarkSpacing);
    setWatermarkOpacity(preset.watermarkOpacity);
    setDestinationType(preset.destinationType || 'customFolder');
    setSubfolder(preset.subfolder || '');
  }, []);

  const currentSettingsObject = useMemo(
    () => ({
      fileFormat,
      jpegQuality,
      tiffBitDepth,
      enableResize,
      resizeMode,
      resizeValue,
      dontEnlarge,
      outputSharpening,
      upscaleMethod,
      keepMetadata,
      preserveTimestamps,
      stripGps,
      exportMasks,
      cutoutMaskId,
      preserveFolders,
      filenameTemplate,
      enableWatermark,
      watermarkPath,
      watermarkAnchor,
      watermarkScale,
      watermarkSpacing,
      watermarkOpacity,
      destinationType,
      subfolder,
    }),
    [
      fileFormat,
      jpegQuality,
      tiffBitDepth,
      enableResize,
      resizeMode,
      resizeValue,
      dontEnlarge,
      outputSharpening,
      upscaleMethod,
      keepMetadata,
      preserveTimestamps,
      stripGps,
      exportMasks,
      preserveFolders,
      filenameTemplate,
      enableWatermark,
      watermarkPath,
      watermarkAnchor,
      watermarkScale,
      watermarkSpacing,
      watermarkOpacity,
      destinationType,
      subfolder,
    ],
  );

  return {
    fileFormat,
    setFileFormat,
    jpegQuality,
    setJpegQuality,
    tiffBitDepth,
    setTiffBitDepth,
    enableResize,
    setEnableResize,
    resizeMode,
    setResizeMode,
    resizeValue,
    setResizeValue,
    dontEnlarge,
    setDontEnlarge,
    outputSharpening,
    setOutputSharpening,
    upscaleMethod,
    setUpscaleMethod,
    keepMetadata,
    setKeepMetadata,
    preserveTimestamps,
    setPreserveTimestamps,
    stripGps,
    setStripGps,
    exportMasks,
    setExportMasks,
    cutoutMaskId,
    setCutoutMaskId,
    preserveFolders,
    setPreserveFolders,
    filenameTemplate,
    setFilenameTemplate,
    enableWatermark,
    setEnableWatermark,
    watermarkPath,
    setWatermarkPath,
    watermarkAnchor,
    setWatermarkAnchor,
    watermarkScale,
    setWatermarkScale,
    watermarkSpacing,
    setWatermarkSpacing,
    watermarkOpacity,
    setWatermarkOpacity,
    destinationType,
    setDestinationType,
    subfolder,
    setSubfolder,
    handleApplyPreset,
    currentSettingsObject,
  };
}
