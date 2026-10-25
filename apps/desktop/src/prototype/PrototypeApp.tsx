/**
 * PROTOTYPE — throwaway. Reached at /?prototype — an exploration surface for the UI redesign.
 * Owns its own tokens (see theme.ts); the seven live panels and src/styles/tokens.css stay untouched.
 *
 * The shell it now models: left project tree · centre is always the chat (no breadcrumb, no panel
 * tabs) · right sidepanel with four context tabs · Skills, MCP and Settings inside a near-fullscreen
 * modal. Picking a side section from the bar shows its full-width specimen in the centre, because the
 * right panel itself is fixed to the chosen compact presentation.
 */

import { useCallback, useState } from "react";
import { SettingsModal, type ModalTab } from "./modal/SettingsModal";
import { NavTree } from "./nav/NavTree";
import { PrototypeSwitcher } from "./PrototypeSwitcher";
import { SidePanel } from "./sidepanel/SidePanel";
import { ThemePanel } from "./ThemePanel";
import {
  chatSection,
  defaultVariantKey,
  findSection,
  findVariant,
  guideComponents,
  guideSections,
  sections,
  type SectionId,
} from "./sections";
import { cssVars, defaultTheme, findPreset, presets, type Density, type RadiusStep, type ThemeState } from "./theme";
import "./prototype.css";
import "./pass2.css";
import "./sidepanel/sidepanel.css";
import "./nav/nav.css";
import "./shell.css";
import "./modal/modal.css";

const radii: RadiusStep[] = [0, 4, 8, 12];
const densities: Density[] = ["compact", "cozy", "comfy"];

const params = new URLSearchParams(window.location.search);

function readInitialTheme(): ThemeState {
  const radius = Number(params.get("radius"));
  const density = params.get("density") as Density | null;
  return {
    presetId: findPreset(params.get("theme") ?? defaultTheme.presetId).id,
    accentId: params.get("accent") ?? defaultTheme.accentId,
    radius: radii.includes(radius as RadiusStep) ? (radius as RadiusStep) : defaultTheme.radius,
    density: density && densities.includes(density) ? density : defaultTheme.density,
  };
}

const initialSection = findSection(params.get("section"));

function modalTabForSection(id: SectionId): ModalTab {
  if (id === "skills") return "catalog";
  if (id === "mcp") return "mcp";
  return "general";
}

function sectionForModalTab(tab: ModalTab): SectionId {
  if (tab === "catalog") return "skills";
  if (tab === "mcp") return "mcp";
  return "settings";
}

export function PrototypeApp() {
  const [section, setSection] = useState<SectionId>(initialSection.id);
  const [theme, setTheme] = useState<ThemeState>(readInitialTheme);
  const [variantKey, setVariantKey] = useState<string>(params.get("variant") ?? defaultVariantKey(initialSection));
  const [sideCollapsed, setSideCollapsed] = useState(false);
  const [modalOpen, setModalOpen] = useState(initialSection.surface === "modal");
  const [modalTab, setModalTab] = useState<ModalTab>(modalTabForSection(initialSection.id));

  const active = findSection(section);
  const variant = findVariant(active, variantKey);

  const skillsVariant = findVariant(findSection("skills"), active.id === "skills" ? variantKey : null);
  const mcpVariant = findVariant(findSection("mcp"), active.id === "mcp" ? variantKey : null);
  const SkillsBody = skillsVariant?.Component;
  const McpBody = mcpVariant?.Component;

  const applyTheme = useCallback((patch: Partial<ThemeState>) => {
    setTheme((current) => {
      const next = { ...current, ...patch };
      return next;
    });
  }, []);

  // The centre shows a full-width specimen unless a modal section is selected — then it is the chat.
  const showSpecimen = active.surface !== "modal";
  const ThemeGuide = () => <ThemePanel theme={theme} onChange={applyTheme} />;
  const Centre = showSpecimen
    ? active.id === "theme"
      ? ThemeGuide
      : active.id === "sessions"
        ? variant?.Component
        : (guideComponents[active.id] ?? variant?.Component)
    : findVariant(chatSection, null)?.Component;

  const go = useCallback((next: SectionId) => {
    const target = findSection(next);
    setSection(next);
    setVariantKey(defaultVariantKey(target));
    if (target.surface === "modal") {
      setModalTab(modalTabForSection(next));
      setModalOpen(true);
    }
  }, []);

  const openModalAt = useCallback((tab: ModalTab) => {
    const next = sectionForModalTab(tab);
    setModalTab(tab);
    setSection(next);
    setVariantKey(defaultVariantKey(findSection(next)));
    setModalOpen(true);
  }, []);

  const pickVariant = useCallback((key: string) => setVariantKey(key), []);

  const cycle = useCallback(
    (delta: number) => {
      if (variant && active.variants) {
        const index = active.variants.findIndex((entry) => entry.key === variant.key);
        const next = active.variants[(index + delta + active.variants.length) % active.variants.length];
        pickVariant(next.key);
        return;
      }
      const index = presets.findIndex((preset) => preset.id === theme.presetId);
      setTheme((current) => ({ ...current, presetId: presets[(index + delta + presets.length) % presets.length].id }));
    },
    [active.variants, pickVariant, theme.presetId, variant],
  );

  const preset = findPreset(theme.presetId);
  const vars = cssVars(theme) as React.CSSProperties;
  const guideActive = active.group === "guide";

  return (
    <div className="pv-root" style={vars} data-testid="prototype-root">
      <NavTree
        activeSession="s1"
        onSettings={() => openModalAt("general")}
        onSkills={() => openModalAt("catalog")}
        onMcp={() => openModalAt("mcp")}
      />

      <main className="pv-main">
        <div className={sideCollapsed ? "pv-workspace is-collapsed" : "pv-workspace"}>
          <div className="pv-centre">
            <div className="pv-chat-head">
              <span className="pv-avatar">CC</span>
              <span className="pv-chat-title" data-testid="chat-title">ACP adapter behind HarnessAdapter</span>
              <span className="pv-chip">
                <span className="pv-status-dot is-live" /> claude-code
              </span>
              <span className="pv-chip">
                <span className="pv-status-dot is-ok" /> codex
              </span>
              <span className="pv-badge is-warning">1 approval</span>
              <div className="pv-chat-actions">
                <button
                  type="button"
                  className="pv-btn sz-sm is-ghost"
                  onClick={() => setSideCollapsed(!sideCollapsed)}
                  title="Toggle sidepanel"
                >
                  ▥
                </button>
              </div>
            </div>

            <div className="pv-content">
              {showSpecimen && active.variants ? (
                <div className="pv-row" style={{ gap: 10 }}>
                  <span className="pv-badge is-accent" data-testid="variant-badge">
                    {variant?.key} · {variant?.name}
                  </span>
                  {variant?.chosen ? <span className="pv-badge is-success">chosen</span> : null}
                  <span className="pv-small pv-faint">{variant?.bet}</span>
                </div>
              ) : null}
              {showSpecimen && guideActive ? (
                <div className="pv-row" style={{ gap: 10 }}>
                  <span className="pv-badge is-warning">style guide</span>
                  <span className="pv-small pv-faint">
                    {active.title} — prototype plumbing, not a product surface
                  </span>
                </div>
              ) : null}

              {Centre ? <Centre /> : null}
            </div>
          </div>

          <SidePanel
            variant="C"
            collapsed={sideCollapsed}
            onToggle={() => setSideCollapsed(!sideCollapsed)}
          />
        </div>

        <div className="pv-statusbar">
          <span>◆ {preset.name}</span>
          <span>{theme.radius === 0 ? "square" : `radius ${theme.radius}px`}</span>
          <span>{theme.density}</span>
          <span className="pv-rail-spacer" style={{ flex: 1 }} />
          <div className="pv-proto-cluster">
            <span className="pv-proto-tag">proto</span>
            <select
              className="pv-select pv-section-select"
              aria-label="Prototype section"
              value={active.id}
              onChange={(event) => go(event.target.value as SectionId)}
            >
              <optgroup label="Style guide">
                {guideSections.map((entry) => (
                  <option key={entry.id} value={entry.id}>
                    {entry.title}
                  </option>
                ))}
              </optgroup>
              <optgroup label="Centre">
                {sections
                  .filter((entry) => entry.surface === "chat")
                  .map((entry) => (
                    <option key={entry.id} value={entry.id}>
                      {entry.title}
                    </option>
                  ))}
              </optgroup>
              <optgroup label="Right panel">
                {sections
                  .filter((entry) => entry.surface === "side")
                  .map((entry) => (
                    <option key={entry.id} value={entry.id}>
                      {entry.title}
                    </option>
                  ))}
              </optgroup>
              <optgroup label="Modal">
                {sections
                  .filter((entry) => entry.surface === "modal")
                  .map((entry) => (
                    <option key={entry.id} value={entry.id}>
                      {entry.title}
                    </option>
                  ))}
              </optgroup>
            </select>
            {active.variants ? (
              <div className="pv-segmented" aria-label="Variant">
                {active.variants.map((entry) => (
                  <button
                    key={entry.key}
                    type="button"
                    className={entry.key === variant?.key ? "pv-seg is-active" : "pv-seg"}
                    onClick={() => pickVariant(entry.key)}
                    title={entry.name}
                  >
                    {entry.key}
                    {entry.chosen ? " ✓" : ""}
                  </button>
                ))}
              </div>
            ) : null}
          </div>
        </div>
      </main>

      <PrototypeSwitcher
        label={variant ? `${variant.key} · ${variant.name}` : `${preset.name} · ${preset.scheme}`}
        hint={
          variant
            ? `${(active.variants?.findIndex((entry) => entry.key === variant.key) ?? 0) + 1}/${active.variants?.length ?? 1}`
            : `${presets.findIndex((entry) => entry.id === preset.id) + 1}/${presets.length}`
        }
        onPrev={() => cycle(-1)}
        onNext={() => cycle(1)}
      />

      {modalOpen ? (
        <SettingsModal
          tab={modalTab}
          onTab={(tab) => openModalAt(tab)}
          onClose={() => setModalOpen(false)}
          theme={theme}
          onTheme={applyTheme}
          catalog={SkillsBody ? <SkillsBody /> : null}
          mcp={McpBody ? <McpBody /> : null}
          variantProps={
            active.id === "skills" && skillsVariant
              ? {
                  variants: findSection("skills").variants ?? [],
                  activeKey: skillsVariant.key,
                  onPick: pickVariant,
                }
              : active.id === "mcp" && mcpVariant
                ? {
                    variants: findSection("mcp").variants ?? [],
                    activeKey: mcpVariant.key,
                    onPick: pickVariant,
                  }
                : null
          }
        />
      ) : null}
    </div>
  );
}
