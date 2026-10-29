import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ProviderGlyph } from "./ProviderGlyph";

describe("ProviderGlyph", () => {
  it.each(["anthropic", "openai", "google", "openrouter", "opencode-go"])(
    "draws the %s mark as a tinted monochrome glyph",
    (provider) => {
      const { container } = render(<ProviderGlyph provider={provider} />);
      const svg = container.querySelector("svg.provider-glyph");
      expect(svg).not.toBeNull();
      expect(svg).toHaveAttribute("viewBox", "0 0 24 24");
      expect(svg).toHaveAttribute("fill", "currentColor");
      expect(container.querySelector(".mark")).toBeNull();
    },
  );

  it("falls back to the initials monogram for a provider it does not know", () => {
    render(<ProviderGlyph provider="acme" />);
    expect(document.querySelector("svg.provider-glyph")).toBeNull();
    expect(screen.getByText("AC")).toHaveClass("mark");
  });
});
