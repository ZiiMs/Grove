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
});
