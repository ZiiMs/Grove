import {
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { Icon } from "../workspace/Icon";

export interface ComboOption {
  value: string;
  label?: string;
  hint?: string;
  /** Listed but unselectable: the keyboard skips it and the row renders dimmed. */
  disabled?: boolean;
  /** Leading glyph, hidden from the accessible name; the label still carries the meaning. */
  icon?: ReactNode;
}

export interface ComboGroup {
  label?: string;
  options: ComboOption[];
}

interface ComboProps {
  id?: string;
  /** Accessible name, also the visible label supplied by the field. */
  label: string;
  value: string;
  groups: ComboGroup[];
  onChange: (value: string) => void;
  searchable?: boolean;
  disabled?: boolean;
  placement?: "up" | "down";
  className?: string;
  /** Extra class on the trigger, for a compact or borderless presentation. */
  triggerClassName?: string;
  title?: string;
  placeholder?: string;
  /** Leading glyph on the trigger, such as the provider mark or the thinking brain. */
  triggerIcon?: ReactNode;
  /** Replaces the plain selected label on the trigger without changing the option text. */
  renderTriggerValue?: (value: string) => ReactNode;
  /** The trigger is a value chip, not an obvious dropdown: hide the caret. */
  hideChevron?: boolean;
  /** Rendered below the list, for an action such as opening provider settings. */
  footer?: ReactNode;
}

function optionLabel(option: ComboOption): string {
  return option.label ?? option.value;
}

/** The nearest row at or after `from` in `step` direction that can be chosen, or -1. */
function selectableFrom(options: ComboOption[], from: number, step: 1 | -1): number {
  for (let index = from; index >= 0 && index < options.length; index += step) {
    if (!options[index].disabled) return index;
  }
  return -1;
}

/** The active row, moved off a disabled one when the list changes under it. */
function resolveActive(options: ComboOption[], active: number): number {
  if (options[active] && !options[active].disabled) return active;
  const after = selectableFrom(options, active, 1);
  return after >= 0 ? after : selectableFrom(options, Math.min(active, options.length - 1), -1);
}

/**
 * A small listbox-based combo. Native selects cannot be searched and look out of place in the
 * composer; this keeps the same label/option meaning while adding filtering for long model lists.
 * Thinking uses the same control with `searchable={false}`.
 */
export function Combo({
  id,
  label,
  value,
  groups,
  onChange,
  searchable = false,
  disabled = false,
  placement = "down",
  className,
  triggerClassName,
  title,
  placeholder = "Choose one",
  triggerIcon,
  renderTriggerValue,
  hideChevron = false,
  footer,
}: ComboProps) {
  const generated = useId();
  const triggerId = id ?? `${generated}-trigger`;
  const listboxId = `${triggerId}-listbox`;
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [active, setActive] = useState(0);

  const baseGroups = useMemo(() => {
    const known = groups.some((group) => group.options.some((option) => option.value === value));
    if (!value || known) return groups;
    return [
      { label: "Not listed", options: [{ value, label: value }] },
      ...groups,
    ];
  }, [groups, value]);

  const visibleGroups = useMemo(() => {
    const query = search.trim().toLowerCase();
    if (!query) return baseGroups;
    return baseGroups
      .map((group) => {
        const groupMatches = Boolean(group.label?.toLowerCase().includes(query));
        return {
          ...group,
          options: groupMatches
            ? group.options
            : group.options.filter((option) =>
                `${optionLabel(option)} ${option.value} ${group.label ?? ""}`
                  .toLowerCase()
                  .includes(query),
              ),
        };
      })
      .filter((group) => group.options.length > 0);
  }, [baseGroups, search]);

  const flatOptions = useMemo(
    () => visibleGroups.flatMap((group) => group.options),
    [visibleGroups],
  );
  const selected =
    baseGroups
      .flatMap((group) => group.options)
      .find((option) => option.value === value) ?? null;
  const selectedLabel = selected ? optionLabel(selected) : value || placeholder;
  const activeIndex = resolveActive(flatOptions, active);
  const activeId =
    open && flatOptions[activeIndex] ? `${triggerId}-option-${activeIndex}` : undefined;

  useLayoutEffect(() => {
    if (!open) return;
    if (searchable) searchRef.current?.focus();
    else listRef.current?.focus();
  }, [open, searchable]);

  useEffect(() => {
    if (!open) return;
    document.getElementById(activeId ?? "")?.scrollIntoView?.({ block: "nearest" });
  }, [activeId, open]);

  useEffect(() => {
    if (!open) return;
    const closeOnOutside = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", closeOnOutside);
    return () => document.removeEventListener("pointerdown", closeOnOutside);
  }, [open]);

  const openMenu = () => {
    const all = baseGroups.flatMap((group) => group.options);
    const current = all.findIndex((option) => option.value === value);
    setSearch("");
    setActive(resolveActive(all, current >= 0 ? current : 0));
    setOpen(true);
  };

  const closeMenu = (focusTrigger = false) => {
    setOpen(false);
    if (focusTrigger) {
      window.requestAnimationFrame(() => triggerRef.current?.focus());
    }
  };

  const choose = (option: ComboOption) => {
    onChange(option.value);
    closeMenu(true);
  };

  const onMenuKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    const typing = event.target instanceof HTMLInputElement;
    const move = (delta: 1 | -1) => {
      const next = selectableFrom(flatOptions, activeIndex + delta, delta);
      if (next >= 0) setActive(next);
    };
    if (event.key === "ArrowDown") {
      event.preventDefault();
      move(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      move(-1);
    } else if (!typing && event.key === "Home") {
      event.preventDefault();
      const first = selectableFrom(flatOptions, 0, 1);
      if (first >= 0) setActive(first);
    } else if (!typing && event.key === "End") {
      event.preventDefault();
      const last = selectableFrom(flatOptions, flatOptions.length - 1, -1);
      if (last >= 0) setActive(last);
    } else if (event.key === "Enter") {
      const option = flatOptions[activeIndex];
      if (option && !option.disabled) {
        event.preventDefault();
        choose(option);
      }
    } else if (event.key === "Escape") {
      event.preventDefault();
      closeMenu(true);
    }
  };

  let optionIndex = -1;

  return (
    <div
      className={["combo", className ?? ""].filter(Boolean).join(" ")}
      ref={rootRef}
      onBlur={(event) => {
        if (!rootRef.current?.contains(event.relatedTarget as Node | null)) setOpen(false);
      }}
    >
      <button
        ref={triggerRef}
        className={["combo-trigger", triggerClassName ?? ""].filter(Boolean).join(" ")}
        id={triggerId}
        type="button"
        role="combobox"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={listboxId}
        aria-activedescendant={activeId}
        title={title ?? label}
        disabled={disabled}
        onClick={() => {
          if (open) closeMenu();
          else openMenu();
        }}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            if (open) closeMenu();
            else openMenu();
          } else if (event.key === "Escape" && open) {
            event.preventDefault();
            closeMenu();
          }
        }}
      >
        {triggerIcon ? (
          <span className="combo-trigger-icon" aria-hidden="true">
            {triggerIcon}
          </span>
        ) : null}
        <span className="combo-value">
          {renderTriggerValue && value ? renderTriggerValue(value) : selectedLabel}
        </span>
        {hideChevron ? null : (
          <span className="combo-chevron" aria-hidden="true">
            <Icon name="chevron" size="xs" />
          </span>
        )}
      </button>

      {open ? (
        <div className={`combo-pop combo-pop-${placement}`}>
          {searchable ? (
            <div className="combo-search">
              <Icon name="search" size="sm" />
              <input
                ref={searchRef}
                type="search"
                aria-label={`Search ${label}`}
                aria-controls={listboxId}
                aria-activedescendant={activeId}
                placeholder={`Search ${label.toLowerCase()}…`}
                value={search}
                onChange={(event) => {
                  setSearch(event.target.value);
                  setActive(0);
                }}
                onKeyDown={onMenuKeyDown}
              />
            </div>
          ) : null}

          <div
            className="combo-list od-scroll"
            id={listboxId}
            role="listbox"
            aria-label={label}
            tabIndex={-1}
            ref={listRef}
            aria-activedescendant={activeId}
            onKeyDown={onMenuKeyDown}
          >
            {visibleGroups.length === 0 ? (
              <p className="combo-empty">No matches.</p>
            ) : (
              visibleGroups.map((group, groupIndex) => (
                <div
                  className="combo-group"
                  key={group.label ?? `group-${groupIndex}`}
                  role={group.label ? "group" : undefined}
                  aria-label={group.label}
                >
                  {group.label ? (
                    <div className="combo-group-label" aria-hidden="true">
                      {group.label}
                    </div>
                  ) : null}
                  {group.options.map((option) => {
                    optionIndex += 1;
                    const index = optionIndex;
                    const selectedOption = option.value === value;
                    return (
                      <button
                        className="combo-option"
                        id={`${triggerId}-option-${index}`}
                        key={option.value}
                        type="button"
                        role="option"
                        aria-selected={selectedOption}
                        data-active={index === activeIndex ? "true" : undefined}
                        disabled={option.disabled}
                        onMouseEnter={() => {
                          if (!option.disabled) setActive(index);
                        }}
                        onClick={() => choose(option)}
                      >
                        {option.icon ? (
                          <span className="combo-option-icon" aria-hidden="true">
                            {option.icon}
                          </span>
                        ) : null}
                        <span className="combo-option-copy">
                          <span>{optionLabel(option)}</span>
                          {option.hint ? <span className="combo-option-hint">{option.hint}</span> : null}
                        </span>
                        {selectedOption ? <Icon name="check" size="sm" /> : null}
                      </button>
                    );
                  })}
                </div>
              ))
            )}
          </div>
          {footer ? <div className="combo-footer">{footer}</div> : null}
        </div>
      ) : null}
    </div>
  );
}
