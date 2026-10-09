export type TextVariant = 'displayLarge' | 'display' | 'headline' | 'title' | 'heading' | 'body' | 'label' | 'small';
export type TextWeight = 'bold' | 'semibold' | 'medium' | 'normal';
export type TextColor = 'primary' | 'secondary' | 'accent' | 'button' | 'info' | 'success' | 'error' | 'white';

export const TextWeights: Record<TextWeight, TextWeight> = {
  bold: 'bold',
  semibold: 'semibold',
  medium: 'medium',
  normal: 'normal',
};
export const TextColors: Record<TextColor, TextColor> = {
  primary: 'primary',
  secondary: 'secondary',
  accent: 'accent',
  button: 'button',
  info: 'info',
  success: 'success',
  error: 'error',
  white: 'white',
};

// Map keys to classes
export const TEXT_WEIGHT_KEYS: Record<TextWeight, string> = {
  bold: 'font-bold',
  semibold: 'font-semibold',
  medium: 'font-medium',
  normal: 'font-normal',
};
export const TEXT_COLOR_KEYS: Record<TextColor, string> = {
  primary: 'text-text-primary',
  secondary: 'text-text-secondary',
  accent: 'text-accent',
  button: 'text-button-text',
  info: 'text-blue-400',
  success: 'text-green-400',
  error: 'text-red-400',
  white: 'text-white',
};

export interface VariantConfig {
  size: string;
  defaultWeight: TextWeight;
  defaultColor: TextColor;
  defaultElement: React.ElementType;
  extraClasses?: string;
}

export const TextVariants: Record<TextVariant, VariantConfig> = {
  displayLarge: {
    size: 'text-4xl',
    defaultWeight: 'normal',
    defaultColor: 'primary',
    defaultElement: 'h1',
    extraClasses: 'tracking-tight mb-3',
  },
  display: {
    size: 'text-2xl',
    defaultWeight: 'semibold',
    defaultColor: 'primary',
    defaultElement: 'h1',
    extraClasses: 'tracking-tight',
  },
  headline: {
    size: 'text-xl',
    defaultWeight: 'semibold',
    defaultColor: 'primary',
    defaultElement: 'h1',
    extraClasses: 'tracking-tight',
  },
  title: {
    size: 'text-base',
    defaultWeight: 'semibold',
    defaultColor: 'primary',
    defaultElement: 'h2',
  },
  heading: {
    size: 'text-[13px]',
    defaultWeight: 'semibold',
    defaultColor: 'primary',
    defaultElement: 'h3',
  },
  body: {
    size: 'text-[13px]',
    defaultWeight: 'normal',
    defaultColor: 'secondary',
    defaultElement: 'p',
  },
  label: {
    size: 'text-xs',
    defaultWeight: 'medium',
    defaultColor: 'secondary',
    defaultElement: 'span',
  },
  small: {
    size: 'text-[11px]',
    defaultWeight: 'normal',
    defaultColor: 'secondary',
    defaultElement: 'p',
  },
};
