import type { ButtonHTMLAttributes, PropsWithChildren } from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../lib/cn";

const button = cva("button", {
  variants: {
    variant: {
      primary: "button--primary",
      secondary: "button--secondary",
      ghost: "button--ghost",
      danger: "button--danger",
    },
    size: { sm: "button--sm", md: "button--md", icon: "button--icon" },
  },
  defaultVariants: { variant: "secondary", size: "md" },
});

type Props = ButtonHTMLAttributes<HTMLButtonElement> & VariantProps<typeof button>;

export function Button({ className, variant, size, children, ...props }: PropsWithChildren<Props>) {
  return (
    <button className={cn(button({ variant, size }), className)} {...props}>
      {children}
    </button>
  );
}
