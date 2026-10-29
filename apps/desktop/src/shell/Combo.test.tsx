import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Combo } from "./Combo";

const groups = [
  {
    label: "Vendor",
    options: [
      { value: "alpha" },
      { value: "beta" },
      { value: "gamma" },
    ],
  },
];

const withDisabled = [
  {
    label: "Vendor",
    options: [
      { value: "alpha" },
      { value: "beta", disabled: true, hint: "Add a key in Settings" },
      { value: "gamma" },
    ],
  },
];

describe("Combo", () => {
  it("filters a searchable list and selects a result", async () => {
    const onChange = vi.fn();
    render(
      <Combo
        label="Model"
        value="alpha"
        groups={groups}
        searchable
        onChange={onChange}
      />,
    );

    await userEvent.click(screen.getByRole("combobox", { name: "Model" }));
    const search = screen.getByRole("searchbox", { name: "Search Model" });
    await userEvent.type(search, "gam");

    expect(screen.getByRole("option", { name: "gamma" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "alpha" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("option", { name: "gamma" }));
    expect(onChange).toHaveBeenCalledWith("gamma");
  });

  it("drives a non-searchable list with the keyboard", async () => {
    const onChange = vi.fn();
    render(
      <Combo
        label="Thinking"
        value="alpha"
        groups={groups}
        onChange={onChange}
      />,
    );

    await userEvent.click(screen.getByRole("combobox", { name: "Thinking" }));
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();
    await userEvent.keyboard("{ArrowDown}{Enter}");

    expect(onChange).toHaveBeenCalledWith("beta");
  });

  it("skips a disabled option with the keyboard and cannot select it", async () => {
    const onChange = vi.fn();
    render(
      <Combo label="Model" value="alpha" groups={withDisabled} onChange={onChange} />,
    );

    await userEvent.click(screen.getByRole("combobox", { name: "Model" }));
    const disabled = screen.getByRole("option", { name: /beta/ });
    expect(disabled).toBeDisabled();
    expect(disabled).toHaveTextContent("Add a key in Settings");

    await userEvent.click(disabled);
    expect(onChange).not.toHaveBeenCalled();

    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("option", { name: "gamma" })).toHaveAttribute("data-active", "true");
    await userEvent.keyboard("{Enter}");
    expect(onChange).toHaveBeenCalledWith("gamma");
  });

  it("shows a trigger icon and a custom trigger value without a chevron", async () => {
    render(
      <Combo
        label="Model"
        value="alpha"
        groups={groups}
        onChange={() => {}}
        hideChevron
        triggerIcon={<svg data-testid="provider-glyph" />}
        renderTriggerValue={(value) => <span>Pretty {value}</span>}
      />,
    );

    const trigger = screen.getByRole("combobox", { name: "Model" });
    expect(trigger).toHaveTextContent("Pretty alpha");
    expect(screen.getByTestId("provider-glyph")).toBeInTheDocument();
    expect(trigger.querySelector(".combo-chevron")).toBeNull();
  });

  it("renders a footer action inside the popup", async () => {
    const onProviders = vi.fn();
    render(
      <Combo
        label="Model"
        value="alpha"
        groups={groups}
        onChange={() => {}}
        footer={<button type="button" onClick={onProviders}>Providers</button>}
      />,
    );

    await userEvent.click(screen.getByRole("combobox", { name: "Model" }));
    await userEvent.click(screen.getByRole("button", { name: "Providers" }));
    expect(onProviders).toHaveBeenCalledOnce();
  });

  it("marks options with their icon", async () => {
    render(
      <Combo
        label="Model"
        value="alpha"
        groups={[
          {
            label: "Vendor",
            options: [{ value: "alpha", icon: <svg data-testid="option-glyph" /> }],
          },
        ]}
        onChange={() => {}}
      />,
    );

    await userEvent.click(screen.getByRole("combobox", { name: "Model" }));
    expect(screen.getByRole("option", { name: "alpha" })).toContainElement(
      screen.getByTestId("option-glyph"),
    );
  });
});
