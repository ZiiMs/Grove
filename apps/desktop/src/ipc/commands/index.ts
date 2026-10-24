import type { AppCommands } from "./app";

/**
 * Every command the webview may invoke, with its argument and result types.
 *
 * Adding a domain: declare `<Domain>Commands` beside this file, then add it to the intersection below.
 */
export type CommandMap = AppCommands;

export type CommandName = keyof CommandMap;
export type CommandArgs<K extends CommandName> = CommandMap[K]["args"];
export type CommandResult<K extends CommandName> = CommandMap[K]["result"];
