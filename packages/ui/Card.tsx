import type { HTMLAttributes, ReactNode } from "react";

import "./Card.css";

type CardProps = HTMLAttributes<HTMLDivElement> & {
  padding?: "regular" | "compact" | "none";
  tone?: "default" | "inset" | "accent";
  as?: "div" | "section" | "article";
};

export function Card({
  padding = "regular",
  tone = "default",
  as: Tag = "div",
  className,
  children,
  ...rest
}: CardProps) {
  return (
    <Tag
      className={[
        "ui-card",
        `ui-card-pad-${padding}`,
        tone !== "default" && `ui-card-${tone}`,
        className,
      ]
        .filter(Boolean)
        .join(" ")}
      {...rest}
    >
      {children}
    </Tag>
  );
}

export function CardHeader({
  title,
  subtitle,
  icon,
  actions,
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  icon?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="ui-card-header">
      {icon && <span className="ui-card-icon">{icon}</span>}
      <div className="ui-card-titles">
        <h3 className="ui-card-title">{title}</h3>
        {subtitle && <p className="ui-card-subtitle">{subtitle}</p>}
      </div>
      {actions && <div className="ui-card-actions">{actions}</div>}
    </div>
  );
}

export function CardFooter({ children }: { children: ReactNode }) {
  return <div className="ui-card-footer">{children}</div>;
}
