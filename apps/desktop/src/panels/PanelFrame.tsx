import type { ReactNode } from "react";

/** Shared panel chrome. Feature tickets replace each panel's body, not this frame. */

export function PanelFrame({
  title,
  subtitle,
  actions,
  children,
}: {
  title: string;
  subtitle?: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="panel" aria-label={title}>
      <header className="panel-header">
        <div>
          <h1 className="panel-title">{title}</h1>
          {subtitle ? <p className="panel-subtitle">{subtitle}</p> : null}
        </div>
        {actions ? <div className="panel-actions">{actions}</div> : null}
      </header>
      <div className="panel-body">{children}</div>
    </section>
  );
}

export function Placeholder({ title, hint }: { title: string; hint: string }) {
  return (
    <PanelFrame title={title} subtitle="Foundation shell">
      <div className="placeholder" data-testid={`placeholder-${title.toLowerCase()}`}>
        <p>{hint}</p>
        <p className="placeholder-note">
          Wired into the shell and the typed IPC contract. The feature ticket replaces this body; the
          navigation entry and test id stay.
        </p>
      </div>
    </PanelFrame>
  );
}
