import {
  Brush,
  BringToFront,
  Circle,
  Cloud,
  Droplet,
  Droplets,
  Eraser,
  SquareMousePointer,
  RectangleHorizontal,
  TriangleRight,
  User,
  Sun,
  Stamp,
  Bandage,
  Spline,
  MopSparkles,
  Crosshair,
  Highlighter,
  Image, Lasso } from 'lucide-react';
import { motion } from 'framer-motion';
import { useTranslation } from 'react-i18next';
import Text from '../../ui/Text';
import { TextWeights } from '../../../types/typography';
import i18n from 'i18next';

export enum Mask {
  AiDepth = 'ai-depth',
  AiForeground = 'ai-foreground',
  AiSky = 'ai-sky',
  AiSubject = 'ai-subject',
  All = 'all',
  Brush = 'brush',
  Flow = 'flow',
  Color = 'color',
  Linear = 'linear',
  Luminance = 'luminance',
  // The shape you drew around something: the selection itself, so nothing has to guess what you meant.
  Polygon = 'polygon',
  QuickEraser = 'quick-eraser',
  Radial = 'radial',
  // Creation-only: a Radial + intersecting Color pair (Snapseed-style selective point).
  Point = 'point',
  // Creation-only: an AI subject mask selected by brushing a stroke (Snapseed Quick Select).
  QuickSelect = 'quick-select',
  // Creation-only: automatic foreground mask, inverted (Lightroom "Select Background").
  AiBackground = 'ai-background',
  Clone = 'clone',
  Heal = 'heal',
  Liquify = 'liquify',
  Retouch = 'retouch',
}

export enum SubMaskMode {
  Additive = 'additive',
  Subtractive = 'subtractive',
  Intersect = 'intersect',
}

export enum ToolType {
  AiSeletor = 'ai-selector',
  Brush = 'brush',
  Eraser = 'eraser',
  GenerativeReplace = 'generative-replace',
  SelectSubject = 'select-subject',
}

export interface MaskType {
  disabled: boolean;
  icon: any;
  id?: string;
  name: string;
  type: Mask;
}

export interface SubMask {
  id: string;
  invert: boolean;
  mode: SubMaskMode;
  name?: string;
  opacity: number;
  parameters?: any;
  type: Mask;
  visible: boolean;
}

export function formatMaskTypeName(type: string) {
  if (type === Mask.AiDepth) return i18n.t('masks.types.depth');
  if (type === Mask.AiSubject) return i18n.t('masks.types.subject');
  if (type === Mask.AiForeground) return i18n.t('masks.types.foreground');
  if (type === Mask.AiSky) return i18n.t('masks.types.sky');
  if (type === Mask.All) return i18n.t('masks.types.all');
  if (type === Mask.QuickEraser) return i18n.t('masks.types.quickEraser');
  if (type === Mask.Brush) return i18n.t('masks.types.brush');
  if (type === Mask.Flow) return i18n.t('masks.types.flow');
  if (type === Mask.Color) return i18n.t('masks.types.color');
  if (type === Mask.Linear) return i18n.t('masks.types.linear');
  if (type === Mask.Luminance) return i18n.t('masks.types.luminance');
  if (type === Mask.Radial) return i18n.t('masks.types.radial');
  if (type === Mask.Polygon) return i18n.t('masks.types.polygon');
  if (type === Mask.Point) return i18n.t('masks.types.point');
  if (type === Mask.QuickSelect) return i18n.t('masks.types.quickSelect');
  if (type === Mask.AiBackground) return i18n.t('masks.types.background');
  if (type === Mask.Clone) return i18n.t('masks.types.clone');
  if (type === Mask.Heal) return i18n.t('masks.types.heal');
  if (type === Mask.Liquify) return i18n.t('masks.types.liquify');
  if (type === Mask.Retouch) return i18n.t('masks.types.retouch');
  return type.charAt(0).toUpperCase() + type.slice(1);
}

export function getMaskTypeName(mask: MaskType) {
  if (mask.id === 'others') return i18n.t('masks.types.others');
  if (mask.type === Mask.QuickEraser && mask.name === 'Quick Erase') {
    return i18n.t('masks.types.quickErase');
  }
  return formatMaskTypeName(mask.type);
}

export function getSubMaskName(subMask: Pick<SubMask, 'name' | 'type'>) {
  return subMask.name?.trim() || formatMaskTypeName(subMask.type);
}

export const MASK_ICON_MAP: Record<Mask, any> = {
  [Mask.Polygon]: Lasso,
  [Mask.AiDepth]: BringToFront,
  [Mask.AiForeground]: User,
  [Mask.AiSky]: Cloud,
  [Mask.AiSubject]: SquareMousePointer,
  [Mask.All]: RectangleHorizontal,
  [Mask.Brush]: Brush,
  [Mask.Flow]: Droplets,
  [Mask.Color]: Droplet,
  [Mask.Linear]: TriangleRight,
  [Mask.Luminance]: Sun,
  [Mask.QuickEraser]: Eraser,
  [Mask.Radial]: Circle,
  [Mask.Point]: Crosshair,
  [Mask.QuickSelect]: Highlighter,
  [Mask.AiBackground]: Image,
  [Mask.Clone]: Stamp,
  [Mask.Heal]: Bandage,
  [Mask.Liquify]: Spline,
  [Mask.Retouch]: MopSparkles,
};

export const MASK_AI_TYPES: Array<MaskType> = [
  { disabled: false, icon: SquareMousePointer, name: 'Subject', type: Mask.AiSubject },
  { disabled: false, icon: Cloud, name: 'Sky', type: Mask.AiSky },
  { disabled: false, icon: Image, name: 'Background', type: Mask.AiBackground },
  { disabled: false, icon: Highlighter, name: 'Quick Select', type: Mask.QuickSelect },
  { disabled: false, icon: User, name: 'Foreground', type: Mask.AiForeground },
  { disabled: false, icon: BringToFront, name: 'Depth', type: Mask.AiDepth },
];

export const MASK_BASIC_TYPES: Array<MaskType> = [
  { disabled: false, icon: Brush, name: 'Brush', type: Mask.Brush },
  { disabled: false, icon: TriangleRight, name: 'Linear', type: Mask.Linear },
  { disabled: false, icon: Circle, name: 'Radial', type: Mask.Radial },
  { disabled: false, icon: Crosshair, name: 'Point', type: Mask.Point },
  { disabled: false, icon: Droplets, name: 'Flow', type: Mask.Flow },
];

export const MASK_RANGE_TYPES: Array<MaskType> = [
  { disabled: false, icon: Droplet, name: 'Color', type: Mask.Color },
  { disabled: false, icon: Sun, name: 'Luminance', type: Mask.Luminance },
  { disabled: false, icon: RectangleHorizontal, name: 'Whole Image', type: Mask.All },
];

export const ALL_MASK_TYPES = [...MASK_AI_TYPES, ...MASK_BASIC_TYPES, ...MASK_RANGE_TYPES];

export const AI_DIRECT_PATCH_TYPES: Array<MaskType> = [
  {
    disabled: false,
    icon: Stamp,
    name: 'Clone',
    type: Mask.Clone,
  },
  {
    disabled: false,
    icon: Bandage,
    name: 'Heal',
    type: Mask.Heal,
  },
];

export const AI_TOUCH_UP_TYPES: Array<MaskType> = [
  {
    disabled: false,
    icon: Spline,
    name: 'Liquify',
    type: Mask.Liquify,
  },
  {
    disabled: false,
    icon: MopSparkles,
    name: 'Retouch',
    type: Mask.Retouch,
  },
  {
    disabled: false,
    icon: Eraser,
    name: 'Quick Erase',
    type: Mask.QuickEraser,
  },
];

export const AI_GENERATIVE_CREATION_TYPES: Array<MaskType> = [
  {
    disabled: false,
    icon: SquareMousePointer,
    name: 'Subject',
    type: Mask.AiSubject,
  },
  {
    disabled: false,
    icon: User,
    name: 'Foreground',
    type: Mask.AiForeground,
  },
  {
    disabled: false,
    icon: Brush,
    name: 'Brush',
    type: Mask.Brush,
  },
  {
    disabled: false,
    icon: TriangleRight,
    name: 'Linear',
    type: Mask.Linear,
  },
  {
    disabled: false,
    icon: Circle,
    name: 'Radial',
    type: Mask.Radial,
  },
];

export const OTHERS_MASK_TYPES: Array<MaskType> = [
  {
    disabled: false,
    icon: BringToFront,
    name: 'Depth',
    type: Mask.AiDepth,
  },
  {
    disabled: false,
    icon: Droplet,
    name: 'Color',
    type: Mask.Color,
  },
  {
    disabled: false,
    icon: Sun,
    name: 'Luminance',
    type: Mask.Luminance,
  },
  {
    disabled: false,
    icon: Brush,
    name: 'Brush',
    type: Mask.Brush,
  },
  {
    disabled: false,
    icon: Droplets,
    name: 'Flow',
    type: Mask.Flow,
  },
  {
    disabled: false,
    icon: RectangleHorizontal,
    name: 'Whole Image',
    type: Mask.All,
  },
];

export const AI_SUB_MASK_COMPONENT_TYPES: Array<MaskType> = [
  ...AI_DIRECT_PATCH_TYPES,
  ...AI_TOUCH_UP_TYPES,
  ...AI_GENERATIVE_CREATION_TYPES,
];

export function NewMaskDropZone({ isOver, textKey }: { isOver: boolean; textKey: string }) {
  const { t } = useTranslation();
  return (
    <motion.div
      layout
      initial={{ opacity: 0, height: 0, marginTop: 0 }}
      animate={{ opacity: 1, height: 'auto', marginTop: '4px' }}
      exit={{ opacity: 0, height: 0, marginTop: 0 }}
      transition={{ duration: 0.2, ease: 'easeOut' }}
      className={`p-3 rounded-lg text-center ${isOver ? 'border border-accent/80 bg-bg-tertiary/50' : ''}`}
    >
      <Text weight={TextWeights.medium}>{t(textKey)}</Text>
    </motion.div>
  );
}
