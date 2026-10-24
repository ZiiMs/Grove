import type { AppCommands } from "./app";
import type { GitCommands } from "./git";
import type { SessionsCommands } from "./sessions";
import type { SettingsCommands } from "./settings";
import type { SkillCommands } from "./skills";
import type { SpecsCommands } from "./specs";

/**
 * Every command the webview may invoke, with its argument and result types.
 *
 * Adding a domain: declare `<Domain>Commands` beside this file, then add it to the intersection below.
 */
export type CommandMap = AppCommands &
  GitCommands &
  SessionsCommands &
  SettingsCommands &
  SkillCommands &
  SpecsCommands;

export type CommandName = keyof CommandMap;
export type CommandArgs<K extends CommandName> = CommandMap[K]["args"];
export type CommandResult<K extends CommandName> = CommandMap[K]["result"];
