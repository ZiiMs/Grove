/**
 * PROTOTYPE — throwaway. Settings as a near-fullscreen modal. It is also the home of the two surfaces
 * that are not chat and not side-panel context: the skill catalog and the MCP inspector.
 */

import { useEffect, type ReactNode } from "react";
import { harnesses, settingsGroups } from "../fixtures";
import { Field } from "../panels/Settings";
import { ThemePanel } from "../ThemePanel";
import type { ThemeState } from "../theme";

export type ModalTab = "general" | "secrets" | "appearance" | "catalog" | "mcp" | "harnesses";

export const modalTabs: { id: ModalTab; label: string; glyph: string; note: string }[] = [
  { id: "general", label: "General", glyph: "⚑", note: "defaults for new sessions" },
  { id: "secrets", label: "Secrets", glyph: "⚿", note: "OS keychain" },
  { id: "appearance", label: "Appearance", glyph: "◐", note: "theme, radius, density" },
  { id: "catalog", label: "Skill catalog", glyph: "✦", note: "catalogs, then skills" },
  { id: "mcp", label: "MCP", glyph: "⬡", note: "servers and tools" },
  { id: "harnesses", label: "Harnesses", glyph: "≣", note: "doctor" },
];

interface VariantProps {
  variants: { key: string; name: string; chosen?: boolean }[];
  activeKey: string;
  onPick: (key: string) => void;
}

interface Props {
  tab: ModalTab;
  onTab: (tab: ModalTab) => void;
  onClose: () => void;
  theme: ThemeState;
  onTheme: (patch: Partial<ThemeState>) => void;
  catalog?: ReactNode;
  mcp?: ReactNode;
  variantProps: VariantProps | null;
}

export function SettingsModal({ tab, onTab, onClose, theme, onTheme, catalog, mcp, variantProps }: Props) {
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const current = modalTabs.find((entry) => entry.id === tab) ?? modalTabs[0];

  return (
    <div className="pv-modal-scrim" onClick={onClose} data-testid="settings-modal">
      <div
        className="pv-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="pv-modal-head">
          <span className="pv-brand-mark">⚑</span>
          <b>Settings</b>
          <span className="pv-faint pv-small">{current.note}</span>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          {variantProps ? (
            <>
              <span className="pv-proto-tag">proto</span>
              <div className="pv-segmented" aria-label="Variant">
                {variantProps.variants.map((entry) => (
                  <button
                    key={entry.key}
                    type="button"
                    className={entry.key === variantProps.activeKey ? "pv-seg is-active" : "pv-seg"}
                    onClick={() => variantProps.onPick(entry.key)}
                    title={entry.name}
                  >
                    {entry.key}
                    {entry.chosen ? " ✓" : ""}
                  </button>
                ))}
              </div>
            </>
          ) : null}
          <button type="button" className="pv-btn sz-sm is-ghost" onClick={onClose} aria-label="Close settings">
            ✕
          </button>
        </div>

        <div className="pv-modal-body">
          <nav className="pv-modal-nav" aria-label="Settings sections">
            {modalTabs.map((entry) => (
              <button
                key={entry.id}
                type="button"
                className={entry.id === tab ? "pv-rail-item is-active" : "pv-rail-item"}
                onClick={() => onTab(entry.id)}
              >
                <span className="pv-rail-glyph">{entry.glyph}</span>
                <span>{entry.label}</span>
              </button>
            ))}
            <div className="pv-rail-spacer" />
            <div className="pv-rail-foot">
              <span>v0.1.0 · windows</span>
              <span>Esc closes</span>
            </div>
          </nav>

          <section className="pv-modal-content">
            {tab === "general" ? (
              <div className="pv-col" style={{ gap: 14, maxWidth: 560 }}>
                <h2 className="pv-title">General</h2>
                {settingsGroups[0].fields.map((field) => (
                  <Field key={field.label} {...field} />
                ))}
              </div>
            ) : null}

            {tab === "secrets" ? (
              <div className="pv-col" style={{ gap: 14, maxWidth: 560 }}>
                <h2 className="pv-title">Secrets</h2>
                <span className="pv-small pv-faint">
                  Stored in the OS keychain through <span className="pv-code">SecretStore</span> — never in
                  settings, fixtures or logs.
                </span>
                {settingsGroups[1].fields.map((field) => (
                  <Field key={field.label} {...field} />
                ))}
              </div>
            ) : null}

            {tab === "appearance" ? <ThemePanel theme={theme} onChange={onTheme} /> : null}

            {tab === "catalog" ? catalog : null}
            {tab === "mcp" ? mcp : null}

            {tab === "harnesses" ? (
              <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
                <div className="pv-row">
                  <h2 className="pv-title">Harnesses</h2>
                  <span className="pv-badge is-success">3 ok</span>
                  <span className="pv-badge is-warning">1 degraded</span>
                  <span className="pv-badge is-danger">1 broken</span>
                  <span className="pv-rail-spacer" style={{ flex: 1 }} />
                  <button type="button" className="pv-btn sz-sm">Run doctor</button>
                </div>
                <div className="pv-doctor">
                  {harnesses.map((harness) => (
                    <div key={harness.id} className="pv-card">
                      <div className="pv-card-head">
                        <span className={`pv-status-dot is-${harness.state}`} />
                        <b>{harness.name}</b>
                        <span className="pv-rail-spacer" style={{ flex: 1 }} />
                        <span className="pv-badge">{harness.adapter}</span>
                      </div>
                      <div className="pv-card-body">
                        <span className="pv-small pv-muted">{harness.detail}</span>
                        <div className="pv-row">
                          <button type="button" className="pv-btn sz-sm">Test</button>
                          <span className="pv-rail-spacer" style={{ flex: 1 }} />
                          <label className="pv-switch" aria-label={`Enable ${harness.name}`}>
                            <input type="checkbox" defaultChecked={harness.state !== "err"} />
                          </label>
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            ) : null}
          </section>
        </div>
      </div>
    </div>
  );
}
