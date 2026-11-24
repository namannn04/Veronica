import { type CSSProperties, type ReactNode, useEffect, useRef, useState } from "react";

import "./Page.css";

export type PageWidth = "readable" | "form" | "fluid";

export function Page({
  width = "readable",
  header,
  children,
  className,
  fill = false,
}: {
  width?: PageWidth;
  header?: ReactNode;
  children: ReactNode;
  className?: string;
  fill?: boolean;
}) {
  const sentinel = useRef<HTMLDivElement>(null);
  const [scrolled, setScrolled] = useState(false);

  useEffect(() => {
    const node = sentinel.current;
    if (!node) {
      return;
    }
    const observer = new IntersectionObserver(([entry]) => setScrolled(!entry.isIntersecting));
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  return (
    <div
      className={["ui-page", `ui-page-${width}`, fill && "ui-page-fill", className]
        .filter(Boolean)
        .join(" ")}
    >
      <div ref={sentinel} className="ui-page-sentinel" aria-hidden="true" />
      {header && (
        <div className="ui-page-header-bar" data-scrolled={scrolled || undefined}>
          <div className="ui-page-column">{header}</div>
        </div>
      )}
      <div className="ui-page-column ui-page-body">{children}</div>
    </div>
  );
}

export function PageHeader({
  title,
  subtitle,
  actions,
  accessory,
  leading,
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
  accessory?: ReactNode;
  leading?: ReactNode;
}) {
  return (
    <header className="ui-page-header">
      <div className="ui-page-header-row">
        {leading && <div className="ui-page-header-leading">{leading}</div>}
        <div className="ui-page-titles">
          <h1 className="ui-page-title">{title}</h1>
          {subtitle && <p className="ui-page-subtitle">{subtitle}</p>}
        </div>
        {actions && <div className="ui-page-actions">{actions}</div>}
      </div>
      {accessory && <div className="ui-page-accessory">{accessory}</div>}
    </header>
  );
}

export function Section({
  title,
  description,
  actions,
  children,
  className,
}: {
  title?: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={["ui-section", className].filter(Boolean).join(" ")}>
      {(title || actions) && (
        <SectionHeader title={title} description={description} actions={actions} />
      )}
      {children}
    </section>
  );
}

export function SectionHeader({
  title,
  description,
  actions,
}: {
  title?: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="ui-section-header">
      <div className="ui-section-titles">
        {title && <h2 className="ui-section-title">{title}</h2>}
        {description && <p className="ui-section-description">{description}</p>}
      </div>
      {actions && <div className="ui-section-actions">{actions}</div>}
    </div>
  );
}

export function Stack({
  gap = "section",
  children,
  className,
}: {
  gap?: "section" | "card" | "control";
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={["ui-stack", `ui-stack-${gap}`, className].filter(Boolean).join(" ")}>
      {children}
    </div>
  );
}

export function Grid({
  min = 240,
  children,
  className,
}: {
  min?: number;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={["ui-grid", className].filter(Boolean).join(" ")}
      style={{ "--grid-min": `${min}px` } as CSSProperties}
    >
      {children}
    </div>
  );
}

export function Columns({
  ratio = "even",
  children,
  className,
}: {
  ratio?: "even" | "wide-start" | "wide-end";
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={["ui-columns", `ui-columns-${ratio}`, className].filter(Boolean).join(" ")}>
      {children}
    </div>
  );
}
