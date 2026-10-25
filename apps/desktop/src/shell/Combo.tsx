import {
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { Icon } from "../workspace/Icon";

export interface ComboOption {
  value: string;
  label?: string;
  hint?: string;
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
  title?: string;
  placeholder?: string;
}

function optionLabel(option: ComboOption): string {
  return option.label ?? option.value;
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
  title,
  placeholder = "Choose one",
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
  const activeId =
    open && flatOptions[active] ? `${triggerId}-option-${active}` : undefined;

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
    const current = baseGroups
      .flatMap((group) => group.options)
      .findIndex((option) => option.value === value);
    setSearch("");
    setActive(current >= 0 ? current : 0);
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
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActive((current) => Math.min(current + 1, Math.max(flatOptions.length - 1, 0)));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive((current) => Math.max(current - 1, 0));
    } else if (!typing && event.key === "Home") {
      event.preventDefault();
      setActive(0);
    } else if (!typing && event.key === "End") {
      event.preventDefault();
      setActive(Math.max(flatOptions.length - 1, 0));
    } else if (event.key === "Enter") {
      const option = flatOptions[active];
      if (option) {
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
        className="combo-trigger"
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
        <span className="combo-value">{selectedLabel}</span>
        <span className="combo-chevron" aria-hidden="true">
          <Icon name="chevron" size="xs" />
        </span>
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
                        data-active={index === active ? "true" : undefined}
                        onMouseEnter={() => setActive(index)}
                        onClick={() => choose(option)}
                      >
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
        </div>
      ) : null}
    </div>
  );
}
