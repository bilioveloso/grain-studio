import clsx from 'clsx';
import type { ButtonHTMLAttributes, ReactNode } from 'react';

interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children' | 'size'> {
  children: ReactNode;
  size?: string;
  variant?: string;
}

const Button = ({
  children,
  onClick,
  disabled,
  className = '',
  size: _size,
  variant: _variant,
  ...props
}: ButtonProps) => {
  const baseClasses = `
    flex items-center justify-center gap-2
    font-medium rounded-md
    text-[13px]
    disabled:opacity-50 disabled:cursor-not-allowed
  `;

  const hasSurfaceBg = className.includes('bg-surface');
  // Same trap as the colours: base padding and a caller's `p-0` carry equal weight, which
  // crushed small icon buttons to zero content size.
  const hasOwnPadding = /(^|\s)p[xytrbl]?-/.test(className);
  const hasOwnBackground = /(^|\s)bg-/.test(className);
  const hasOwnTextColor = /(^|\s)text-(?!(xs|sm|base|md|lg|xl|\d|\[|left|right|center|justify|nowrap|wrap))/.test(
    className,
  );

  const combinedClasses = clsx(
    baseClasses,
    {
      'bg-accent': !hasOwnBackground && !hasSurfaceBg,
      'bg-surface': !hasOwnBackground && hasSurfaceBg,
      'text-button-text': !hasOwnTextColor,
      'py-1.5 px-3': !hasOwnPadding,
    },
    className,
  );

  return (
    <button onClick={onClick} disabled={disabled} className={combinedClasses} {...props}>
      {children}
    </button>
  );
};

export default Button;
