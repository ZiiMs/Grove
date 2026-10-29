import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useUiStore } from "../state/uiStore";
import { Toast } from "./Toast";

describe("Toast", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    useUiStore.setState({ toast: null });
  });

  it("keeps a failure message on screen for four seconds", () => {
    useUiStore.setState({
      toast: {
        message: "Couldn't name this session with Haiku 4.5 on Anthropic: rate limited",
        tone: "error",
      },
    });
    render(<Toast />);
    expect(screen.getByRole("status")).toHaveTextContent("Couldn't name this session");

    act(() => {
      vi.advanceTimersByTime(3999);
    });
    expect(useUiStore.getState().toast).not.toBeNull();

    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(useUiStore.getState().toast).toBeNull();
  });
});
