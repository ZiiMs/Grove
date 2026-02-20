# Keybind Footer Cleanup Plan

## Current State

The footer (`src/ui/components/status_bar.rs`) shows 11 keybinds at all times:

```
[n] new  [d] del  [Enter] attach  [s] summary  [m] merge  [p] push  [a] asana  [N] note  [R] refresh  [?] help  [q] quit
```

## Problems

1. **Too many options** - 11 keybinds is overwhelming for a status bar
2. **Mixed priority** - Critical actions (quit, help, new) mixed with infrequent ones (note, asana)
3. **No visual hierarchy** - All keybinds shown equally prominently

## Decision

**Chosen approach:** Minimal Core
**Include delete:** Yes
**Include settings:** Yes

Final keybinds for footer:

```
[n] new  [Enter] attach  [d] delete  [S] settings  [?] help  [q] quit
```

## Implementation Steps

1. **Edit `src/ui/components/status_bar.rs`**
   - Reduce `shortcuts` vector from 11 items to 6 items:
     ```rust
     let shortcuts = vec![
         ("n", "new"),
         ("Enter", "attach"),
         ("d", "delete"),
         ("S", "settings"),
         ("?", "help"),
         ("q", "quit"),
     ];
     ```

2. **Test the change**
   - Run `cargo run`
   - Verify footer displays correctly
   - Verify help overlay still shows all keybinds

## Removed from Footer (still in help)

- `s` - Request summary
- `m` - Merge main
- `p` - Push
- `a` - Assign Asana
- `N` - Set note
- `R` - Refresh all
