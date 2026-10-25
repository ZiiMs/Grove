/**
 * PROTOTYPE — throwaway. Settings panel: sectioned form vs single page vs doctor-first.
 */

import { harnesses, settingsGroups } from "../fixtures";
import type { VariantDef } from "./types";

export function Field({ label, kind, value }: { label: string; kind: string; value: string }) {
  return (
    <div className="pv-field">
      <span className="pv-label">{label}</span>
      {kind === "switch" ? (
        <label className="pv-switch">
          <input type="checkbox" defaultChecked={value === "on"} /> {value === "on" ? "Enabled" : "Disabled"}
        </label>
      ) : kind === "secret" ? (
        <div className="pv-row">
          <input className="pv-input" defaultValue={value} style={{ flex: 1 }} readOnly />
          <span className="pv-badge is-success">keychain</span>
        </div>
      ) : kind === "empty" ? (
        <div className="pv-row">
          <input className="pv-input" placeholder="not set" style={{ flex: 1 }} readOnly />
          <button type="button" className="pv-btn sz-sm">Add</button>
        </div>
      ) : kind === "select" ? (
        <select className="pv-select" defaultValue={value}>
          <option>{value}</option>
        </select>
      ) : (
        <input className="pv-input" defaultValue={value} />
      )}
    </div>
  );
}

/* --------------------------------------------------------------- A — sectioned */

function SectionedLayout() {
  return (
    <div className="pv-settings">
      <div className="pv-settings-nav">
        <div className="pv-rail-group" style={{ padding: "4px 8px" }}>Settings</div>
        {settingsGroups.map((group, index) => (
          <button key={group.id} type="button" className={index === 0 ? "pv-rail-item is-active" : "pv-rail-item"}>
            {group.title}
          </button>
        ))}
      </div>
      <div className="pv-pane">
        <div className="pv-pane-head">
          <b>{settingsGroups[0].title}</b>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <button type="button" className="pv-btn sz-sm is-primary">Save</button>
        </div>
        <div className="pv-scroll pv-col" style={{ gap: 14, padding: "var(--pad-x)", maxWidth: 520 }}>
          {settingsGroups[0].fields.map((field) => (
            <Field key={field.label} {...field} />
          ))}
        </div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------ B — single page */

function SinglePageLayout() {
  return (
    <div className="pv-col" style={{ maxWidth: 640, margin: "0 auto", gap: "var(--pad-x)" }}>
      <div className="pv-anchors">
        {settingsGroups.map((group) => (
          <button key={group.id} type="button" className="pv-chip">{group.title}</button>
        ))}
      </div>
      {settingsGroups.map((group) => (
        <div key={group.id} className="pv-card">
          <div className="pv-card-head">
            <b>{group.title}</b>
          </div>
          <div className="pv-card-body">
            {group.fields.map((field) => (
              <Field key={field.label} {...field} />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

/* --------------------------------------------------------------- C — doctor */

function DoctorLayout() {
  return (
    <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
      <div className="pv-row">
        <span className="pv-title" style={{ fontSize: "var(--font-lg)" }}>Harnesses</span>
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
                <button type="button" className="pv-btn sz-sm is-ghost">Configure</button>
                <span className="pv-rail-spacer" style={{ flex: 1 }} />
                <label className="pv-switch" aria-label={`Enable ${harness.name}`}>
                  <input type="checkbox" defaultChecked={harness.state !== "err"} />
                </label>
              </div>
            </div>
          </div>
        ))}
      </div>

      <div className="pv-grid-2">
        <div className="pv-card">
          <div className="pv-card-head"><b>Secrets</b><span className="pv-badge is-success">OS keychain</span></div>
          <div className="pv-card-body">
            {settingsGroups[1].fields.map((field) => (
              <Field key={field.label} {...field} />
            ))}
          </div>
        </div>
        <div className="pv-card">
          <div className="pv-card-head"><b>Appearance</b></div>
          <div className="pv-card-body">
            {settingsGroups[2].fields.map((field) => (
              <Field key={field.label} {...field} />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

export const settingsVariants: VariantDef[] = [
  { key: "A", name: "Sectioned", chosen: true, bet: "Nav plus one form — scales to 30 settings without a scroll hunt.", Component: SectionedLayout },
  { key: "B", name: "Single page", bet: "Everything on one scroll with anchors — searchable with browser find, no hidden state.", Component: SinglePageLayout },
  { key: "C", name: "Doctor first", bet: "Harness health is the first screen — settings exist to serve five adapters that break independently.", Component: DoctorLayout },
];
