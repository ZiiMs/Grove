/**
 * PROTOTYPE — throwaway. Theme selector: four presets plus accent / radius / density knobs.
 */

import { accents, densitySteps, presets, type RadiusStep, type ThemeState } from "./theme";

const swatchKeys = ["--bg", "--bg-raised", "--border", "--ink-muted", "--ink", "--accent", "--success", "--danger"];
const radii: RadiusStep[] = [0, 4, 8, 12];

interface Props {
  theme: ThemeState;
  onChange: (next: Partial<ThemeState>) => void;
}

export function ThemePanel({ theme, onChange }: Props) {
  return (
    <div className="pv-theme">
      <div className="pv-block">
        <div className="pv-block-head">
          <h2>Presets</h2>
          <span>four directions — flip with ← → or the bar at the bottom</span>
        </div>
        <div className="pv-presets">
          {presets.map((preset) => (
            <button
              key={preset.id}
              type="button"
              className={preset.id === theme.presetId ? "pv-preset is-active" : "pv-preset"}
              onClick={() => onChange({ presetId: preset.id })}
            >
              <span className="pv-swatches">
                {swatchKeys.map((key) => (
                  <i key={key} style={{ background: preset.tokens[key] }} />
                ))}
              </span>
              <span className="pv-preset-name">
                {preset.name}
                <span className="pv-badge" style={{ marginLeft: "auto" }}>
                  {preset.scheme}
                </span>
              </span>
              <span className="pv-preset-blurb">{preset.blurb}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="pv-grid-2">
        <div className="pv-block">
          <div className="pv-block-head">
            <h2>Knobs</h2>
            <span>what a token system has to survive</span>
          </div>
          <div className="pv-card">
            <div className="pv-card-body">
              <div className="pv-field">
                <span className="pv-label">Accent</span>
                <div className="pv-knob-row">
                  <button
                    type="button"
                    className={theme.accentId === "preset" ? "pv-btn sz-sm is-outline" : "pv-btn sz-sm"}
                    onClick={() => onChange({ accentId: "preset" })}
                  >
                    preset
                  </button>
                  {accents.map((accent) => (
                    <button
                      key={accent.id}
                      type="button"
                      aria-label={accent.name}
                      title={accent.name}
                      className={theme.accentId === accent.id ? "pv-accent-sw is-active" : "pv-accent-sw"}
                      style={{ background: accent.dark }}
                      onClick={() => onChange({ accentId: accent.id })}
                    />
                  ))}
                </div>
              </div>

              <div className="pv-field">
                <span className="pv-label">Radius</span>
                <div className="pv-segmented">
                  {radii.map((step) => (
                    <button
                      key={step}
                      type="button"
                      className={theme.radius === step ? "pv-seg is-active" : "pv-seg"}
                      onClick={() => onChange({ radius: step })}
                    >
                      {step}px
                    </button>
                  ))}
                </div>
              </div>

              <div className="pv-field">
                <span className="pv-label">Density</span>
                <div className="pv-segmented">
                  {densitySteps.map((step) => (
                    <button
                      key={step.id}
                      type="button"
                      className={theme.density === step.id ? "pv-seg is-active" : "pv-seg"}
                      onClick={() => onChange({ density: step.id })}
                      title={step.blurb}
                    >
                      {step.name}
                    </button>
                  ))}
                </div>
                <span className="pv-small pv-faint">
                  {densitySteps.find((step) => step.id === theme.density)?.blurb}
                </span>
              </div>
            </div>
          </div>
        </div>

        <div className="pv-block">
          <div className="pv-block-head">
            <h2>Specimen</h2>
            <span>the same three controls under every setting</span>
          </div>
          <div className="pv-card">
            <div className="pv-card-body">
              <div className="pv-row">
                <button type="button" className="pv-btn is-primary">
                  Primary
                </button>
                <button type="button" className="pv-btn">Secondary</button>
                <button type="button" className="pv-btn is-ghost">
                  Ghost
                </button>
                <span className="pv-badge is-accent">accent</span>
                <span className="pv-badge is-success">pass</span>
              </div>
              <div className="pv-input-wrap">
                <span className="pv-faint">⌕</span>
                <input className="pv-input" placeholder="Radius and density follow through…" />
              </div>
              <div className="pv-row">
                <span className="pv-avatar">CC</span>
                <span className="pv-chip">claude-code</span>
                <span className="pv-kbd">⌘K</span>
                <span className="pv-rail-spacer" />
                <span className="pv-status-dot is-live" />
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
